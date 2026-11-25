//! Dispatch of remote commands to the same functions the desktop frontend invokes through Tauri.
//!
//! Arguments use the same names as the frontend's `invoke` calls (camelCase, e.g. `oldId`), and snake_case is accepted too.
//! Commands that only make sense on the machine running OpenDeck (opening folders, file dialogs, local file installs) are not exposed.

use crate::events::frontend;
use crate::shared::{Action, ActionContext, ActionState, Context, DEVICES};

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

fn to_camel(name: &str) -> String {
	let mut out = String::new();
	let mut upper = false;
	for c in name.chars() {
		if c == '_' {
			upper = true;
		} else if upper {
			out.extend(c.to_uppercase());
			upper = false;
		} else {
			out.push(c);
		}
	}
	out
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
	let value = args.get(to_camel(name)).or_else(|| args.get(name)).cloned().unwrap_or(Value::Null);
	serde_json::from_value(value).map_err(|error| format!("invalid argument `{name}`: {error}"))
}

fn ok<T: serde::Serialize>(value: T) -> Result<Value, String> {
	serde_json::to_value(value).map_err(|error| error.to_string())
}

fn err(error: impl std::fmt::Display) -> String {
	error.to_string()
}

fn valid_plugin_id(id: &str) -> bool {
	!id.is_empty() && id != "." && id != ".." && !id.ends_with('.') && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Ask the desktop frontend to reload the selected profile, so it re-renders key images onto the device.
fn refresh_desktop(app: &AppHandle) {
	if let Some(window) = app.get_webview_window("main") {
		let _ = window.emit("rerender_images", ());
	}
}

/// Commands that change a profile's contents and need the desktop frontend to re-render.
const MUTATING: &[&str] = &[
	"create_instance",
	"move_instance",
	"remove_instance",
	"set_state",
	"set_child_delay",
	"set_slot_replicated",
	"set_device_layout",
];

pub async fn dispatch(app: &AppHandle, cmd: &str, args: Value) -> Result<Value, String> {
	let result = run(app, cmd, &args).await;
	if result.is_ok() && MUTATING.contains(&cmd) {
		refresh_desktop(app);
		super::broadcast("profile_changed", json!({ "cmd": cmd }));
	}
	result
}

async fn run(app: &AppHandle, cmd: &str, args: &Value) -> Result<Value, String> {
	match cmd {
		// General
		"get_devices" => ok(frontend::get_devices().await),
		"get_port_base" => ok(frontend::get_port_base().await),
		"get_categories" => ok(frontend::get_categories().await),
		"get_localisations" => ok(frontend::get_localisations(&arg::<String>(args, "locale")?).await.map_err(err)?),
		"get_device_layout" => ok(frontend::get_device_layout(arg(args, "device")?).await),
		"set_device_layout" => ok(frontend::set_device_layout(arg(args, "device")?, arg(args, "layout")?).await.map_err(err)?),
		"get_fonts" => ok(frontend::get_fonts()),
		"get_build_info" => ok(frontend::settings::get_build_info()),

		// Instances
		"create_instance" => {
			let action: Action = arg(args, "action")?;
			let context: Context = arg(args, "context")?;
			ok(frontend::instances::create_instance(app.clone(), action, context).await.map_err(err)?)
		}
		"move_instance" => ok(frontend::instances::move_instance(arg(args, "source")?, arg(args, "destination")?, arg(args, "retain")?)
			.await
			.map_err(err)?),
		"remove_instance" => ok(frontend::instances::remove_instance(arg::<ActionContext>(args, "context")?).await.map_err(err)?),
		"set_state" => ok(frontend::instances::set_state(arg(args, "context")?, arg(args, "index")?, arg::<ActionState>(args, "state")?)
			.await
			.map_err(err)?),
		"set_child_delay" => ok(frontend::instances::set_child_delay(arg(args, "parent_context")?, arg(args, "index")?, arg(args, "delay_ms")?)
			.await
			.map_err(err)?),
		"trigger_virtual_press" => ok(frontend::instances::trigger_virtual_press(arg(args, "context")?).await.map_err(err)?),

		// Profiles
		"get_profiles" => ok(frontend::profiles::get_profiles(&arg::<String>(args, "device")?).await.map_err(err)?),
		"set_profile_order" => ok(frontend::profiles::set_profile_order(arg(args, "device")?, arg(args, "order")?).await.map_err(err)?),
		"get_replicated_slots" => ok(frontend::profiles::get_replicated_slots(arg(args, "device")?).await.map_err(err)?),
		"set_slot_replicated" => ok(frontend::profiles::set_slot_replicated(app.clone(), arg(args, "context")?, arg(args, "replicated")?)
			.await
			.map_err(err)?),
		"get_selected_profile" => ok(frontend::profiles::get_selected_profile(arg(args, "device")?).await.map_err(err)?),
		"set_selected_profile" => ok(set_selected_profile(app, arg(args, "device")?, arg(args, "id")?).await?),
		"delete_profile" => {
			let device: String = arg(args, "device")?;
			let profile: String = arg(args, "profile")?;
			let selected = crate::store::profiles::DEVICE_STORES.write().await.get_selected_profile(&device).map_err(err)?;
			if selected == profile {
				return Err("cannot delete the selected profile; switch to another profile first".to_owned());
			}
			frontend::profiles::delete_profile(device, profile).await;
			super::broadcast("profiles_changed", json!({}));
			ok(())
		}
		"rename_profile" => {
			frontend::profiles::rename_profile(arg(args, "device")?, arg(args, "old_id")?, arg(args, "new_id")?, arg(args, "retain")?)
				.await
				.map_err(err)?;
			super::broadcast("profiles_changed", json!({}));
			ok(())
		}

		// Property inspectors
		"make_info" => ok(frontend::property_inspector::make_info(arg(args, "plugin")?).await.map_err(err)?),
		"switch_property_inspector" => {
			frontend::property_inspector::switch_property_inspector(arg(args, "old")?, arg(args, "new")?).await;
			ok(())
		}

		// Plugins
		"list_plugins" => ok(frontend::plugins::list_plugins(app.clone()).await.map_err(err)?),
		"install_plugin" => {
			let url: Option<String> = arg(args, "url")?;
			if url.is_none() {
				return Err("remote installs need a `url`".to_owned());
			}
			ok(frontend::plugins::install_plugin(app.clone(), url, None, arg(args, "fallback_id")?).await.map_err(err)?)
		}
		"remove_plugin" => ok(frontend::plugins::remove_plugin(app.clone(), arg(args, "id")?).await.map_err(err)?),
		"reload_plugin" => {
			frontend::plugins::reload_plugin(app.clone(), arg(args, "id")?).await;
			ok(())
		}
		"get_plugin_global_settings" => ok(get_plugin_global_settings(&arg::<String>(args, "plugin")?).await?),
		"set_plugin_global_settings" => ok(set_plugin_global_settings(&arg::<String>(args, "plugin")?, arg(args, "settings")?).await?),

		// Settings
		"get_settings" => ok(frontend::settings::get_settings().await),
		"set_settings" => ok(frontend::settings::set_settings(app.clone(), arg(args, "settings")?).await.map_err(err)?),

		// Remote only
		"get_rendered_images" => ok(super::rendered_images(&arg::<String>(args, "device")?).await),
		"get_remote_info" => ok(json!({
			"api": super::API_VERSION,
			"version": crate::built_info::PKG_VERSION,
			"port_base": *crate::plugins::PORT_BASE,
		})),

		_ => Err(format!("unknown or unsupported command `{cmd}`")),
	}
}

/// Switch profiles through the desktop frontend when it shows this device, so that its view and the device stay in step,
/// falling back to switching directly.
async fn set_selected_profile(app: &AppHandle, device: String, id: String) -> Result<(), String> {
	if !DEVICES.contains_key(&device) {
		return Err(format!("device {device} not found"));
	}
	if id.trim().is_empty() {
		return Err("profile id must not be empty".to_owned());
	}

	if let Some(window) = app.get_webview_window("main") {
		let _ = window.emit("switch_profile", json!({ "device": device, "profile": id }));
		for _ in 0..30 {
			tokio::time::sleep(std::time::Duration::from_millis(100)).await;
			if crate::store::profiles::DEVICE_STORES.write().await.get_selected_profile(&device).ok().as_ref() == Some(&id) {
				super::broadcast("profiles_changed", json!({}));
				return Ok(());
			}
		}
		log::warn!("Desktop frontend did not switch profile for remote request; switching directly");
	}

	frontend::profiles::set_selected_profile(device.clone(), id.clone()).await.map_err(err)?;
	refresh_desktop(app);
	super::broadcast("profiles_changed", json!({}));
	Ok(())
}

async fn get_plugin_global_settings(plugin: &str) -> Result<Value, String> {
	if !valid_plugin_id(plugin) {
		return Err("invalid plugin ID".to_owned());
	}
	let path = crate::shared::config_dir().join("settings").join(format!("{plugin}.json"));
	match tokio::fs::read(&path).await {
		Ok(contents) => serde_json::from_slice(&contents).map_err(err),
		Err(_) => Ok(json!({})),
	}
}

/// Write a plugin's global settings, as `setGlobalSettings` would, then notify the plugin and its property inspectors.
async fn set_plugin_global_settings(plugin: &str, settings: Value) -> Result<(), String> {
	use std::io::Write;
	if !valid_plugin_id(plugin) {
		return Err("invalid plugin ID".to_owned());
	}
	let settings_dir = crate::shared::config_dir().join("settings");
	tokio::fs::create_dir_all(&settings_dir).await.map_err(err)?;
	{
		let mut file = std::fs::OpenOptions::new()
			.write(true)
			.truncate(true)
			.create(true)
			.open(settings_dir.join(format!("{plugin}.json")))
			.map_err(err)?;
		file.lock().map_err(err)?;
		file.write_all(settings.to_string().as_bytes()).map_err(err)?;
		file.sync_data().map_err(err)?;
		file.unlock().map_err(err)?;
	}
	let _ = crate::events::outbound::settings::did_receive_global_settings(plugin, false).await;
	let _ = crate::events::outbound::settings::did_receive_global_settings(plugin, true).await;
	Ok(())
}
