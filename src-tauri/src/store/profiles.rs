use super::Store;

use crate::shared::{ActionInstance, DEVICES, DeviceInfo, Profile, config_dir, copy_dir, initialise_encoder_layout};

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub struct ProfileStores {
	stores: HashMap<String, Store<Profile>>,
}

impl ProfileStores {
	fn canonical_id(device: &str, id: &str) -> String {
		if cfg!(target_os = "windows") {
			PathBuf::from(device).join(id.replace('/', "\\")).to_str().unwrap().to_owned()
		} else {
			PathBuf::from(device).join(id).to_str().unwrap().to_owned()
		}
	}

	pub fn get_profile_store(&self, device: &DeviceInfo, id: &str) -> Result<&Store<Profile>, anyhow::Error> {
		self.stores.get(&Self::canonical_id(&device.id, id)).ok_or_else(|| anyhow!("profile not found"))
	}

	pub async fn get_profile_store_mut(&mut self, device: &DeviceInfo, id: &str) -> Result<&mut Store<Profile>, anyhow::Error> {
		let canonical_id = Self::canonical_id(&device.id, id);
		if self.stores.contains_key(&canonical_id) {
			Ok(self.stores.get_mut(&canonical_id).unwrap())
		} else {
			let default = Profile {
				id: id.to_owned(),
				keys: Vec::new(),
				sliders: Vec::new(),
				infobars: Vec::new(),

				stale: false,
			};

			let mut store = Store::new(&canonical_id, &config_dir().join("profiles"), default).context(format!("Failed to create store for profile {}", canonical_id))?;
			store.value.keys.resize((device.rows * device.columns + device.touchpoints) as usize, None);
			store.value.sliders.resize(device.encoders as usize, None);
			store.value.infobars.resize(device.infobars as usize, None);

			let categories = crate::shared::CATEGORIES.read().await;
			let actions = categories.values().flat_map(|v| v.actions.iter()).collect::<Vec<_>>();
			let plugins_dir = config_dir().join("plugins");
			let registered = crate::events::registered_plugins().await;
			let keep_instance = |instance: &ActionInstance| -> bool {
				instance.action.plugin == "opendeck"
					|| (plugins_dir.join(&instance.action.plugin).exists() && (!registered.contains(&instance.action.plugin) || actions.iter().any(|v| v.uuid == instance.action.uuid)))
			};
			for slot in store.value.keys.iter_mut().chain(store.value.sliders.iter_mut()).chain(store.value.infobars.iter_mut()) {
				if let Some(instance) = slot {
					if !keep_instance(instance) {
						*slot = None;
					} else if let Some(children) = &mut instance.children {
						children.retain_mut(|child| keep_instance(child));
					}
				}
			}

			// We need to populate instances from a profile without encoders or without parsed layouts with them
			for instance in store.value.sliders.iter_mut().flatten() {
				// Populate encoder data using the manifest action if missing
				if instance.action.encoder.is_none()
					&& let Some(action) = actions.iter().find(|a| a.uuid == *instance.action.uuid)
				{
					instance.action.encoder = action.encoder.clone();
				}

				// Load encoder layout if not yet parsed
				let _ = initialise_encoder_layout(&mut instance.action, None);
			}

			store.save()?;

			self.stores.insert(canonical_id.clone(), store);
			Ok(self.stores.get_mut(&canonical_id).unwrap())
		}
	}

	pub fn remove_profile(&mut self, device: &str, id: &str) {
		self.stores.remove(&Self::canonical_id(device, id));
	}

	pub fn delete_profile(&mut self, device: &str, id: &str) {
		self.remove_profile(device, id);
		let config_dir = config_dir();
		#[cfg(target_os = "windows")]
		let id = &id.replace('/', "\\");
		let path = config_dir.join("profiles").join(device).join(format!("{id}.json"));
		let _ = fs::remove_file(&path);
		// This is safe as `remove_dir` errors if the directory is not empty.
		let _ = fs::remove_dir(path.parent().unwrap());
		let images_path = config_dir.join("images").join(device).join(id);
		let _ = fs::remove_dir_all(images_path);
	}

	pub async fn rename_profile(&mut self, device: &DeviceInfo, old_id: &str, new_id: &str, retain: bool) -> Result<(), anyhow::Error> {
		if !retain {
			// Remove from the store but don't delete the file
			self.remove_profile(&device.id, old_id);
		}

		let config_dir = config_dir();

		// Construct old and new paths (handling Windows path separators)
		#[cfg(target_os = "windows")]
		let old_path_id = old_id.replace('/', "\\");
		#[cfg(not(target_os = "windows"))]
		let old_path_id = old_id;

		#[cfg(target_os = "windows")]
		let new_path_id = new_id.replace('/', "\\");
		#[cfg(not(target_os = "windows"))]
		let new_path_id = new_id;

		let old_path = config_dir.join("profiles").join(&device.id).join(format!("{}.json", old_path_id));
		let new_path = config_dir.join("profiles").join(&device.id).join(format!("{}.json", new_path_id));

		// Create parent directory for new path if it doesn't exist
		if let Some(parent) = new_path.parent() {
			fs::create_dir_all(parent)?;
		}

		// Rename the profile file
		if !retain {
			fs::rename(&old_path, &new_path)?;

			// Clean up empty old directory if profile was in a folder
			if let Some(parent) = old_path.parent() {
				// This is safe as `remove_dir` errors if the directory is not empty.
				let _ = fs::remove_dir(parent);
			}
		} else {
			fs::copy(&old_path, &new_path)?;
		}

		// Rename images directory if it exists
		let old_images_path = config_dir.join("images").join(&device.id).join(old_path_id);
		let new_images_path = config_dir.join("images").join(&device.id).join(new_path_id);

		if old_images_path.exists() {
			if let Some(parent) = new_images_path.parent() {
				fs::create_dir_all(parent)?;
			}

			if !retain {
				fs::rename(&old_images_path, &new_images_path)?;

				// Clean up empty old images directory
				if let Some(parent) = old_images_path.parent() {
					// This is safe as `remove_dir` errors if the directory is not empty.
					let _ = fs::remove_dir(parent);
				}
			} else {
				copy_dir(&old_images_path, &new_images_path)?;
			}
		}

		// Reload the new profile
		self.get_profile_store_mut(device, new_id).await?;

		Ok(())
	}

	pub fn all_from_plugin(&self, plugin: &str) -> Vec<crate::shared::ActionContext> {
		let mut all = vec![];
		for store in self.stores.values() {
			for instance in store.value.keys.iter().chain(&store.value.sliders).chain(&store.value.infobars).flatten() {
				if instance.action.plugin == plugin {
					all.push(instance.context.clone());
				} else if let Some(children) = &instance.children {
					for child in children {
						if child.action.plugin == plugin {
							all.push(child.context.clone());
						}
					}
				}
			}
		}
		all
	}
}

#[derive(Serialize, Deserialize)]
pub struct DeviceConfig {
	pub selected_profile: String,
	/// User-chosen display order of this device's profiles. Profiles not listed here follow in alphabetical order.
	#[serde(default)]
	pub profile_order: Vec<String>,
	/// Slots (e.g. `Keypad.3`, `Encoder.0`) whose action is kept identical on every profile of this device.
	#[serde(default)]
	pub replicated: Vec<String>,
}

impl super::NotProfile for DeviceConfig {}

pub struct DeviceStores {
	stores: HashMap<String, Store<DeviceConfig>>,
}

impl DeviceStores {
	fn config_store(&mut self, device: &str) -> Result<&mut Store<DeviceConfig>, anyhow::Error> {
		if !self.stores.contains_key(device) {
			let default = DeviceConfig {
				selected_profile: "Default".to_owned(),
				profile_order: Vec::new(),
				replicated: Vec::new(),
			};

			let store = Store::new(device, &config_dir().join("profiles"), default).context(format!("Failed to create store for device config {}", device))?;
			store.save()?;

			self.stores.insert(device.to_owned(), store);
		}
		Ok(self.stores.get_mut(device).unwrap())
	}

	pub fn get_profile_order(&mut self, device: &str) -> Result<Vec<String>, anyhow::Error> {
		Ok(self.config_store(device)?.value.profile_order.clone())
	}

	pub fn set_profile_order(&mut self, device: &str, order: Vec<String>) -> Result<(), anyhow::Error> {
		let store = self.config_store(device)?;
		store.value.profile_order = order;
		store.save()
	}

	pub fn get_replicated(&mut self, device: &str) -> Result<Vec<String>, anyhow::Error> {
		Ok(self.config_store(device)?.value.replicated.clone())
	}

	pub fn set_replicated(&mut self, device: &str, replicated: Vec<String>) -> Result<(), anyhow::Error> {
		let store = self.config_store(device)?;
		store.value.replicated = replicated;
		store.save()
	}

	/// All profiles of a device, in the user's chosen order.
	pub fn get_ordered_profiles(&mut self, device: &str) -> Result<Vec<String>, anyhow::Error> {
		let order = self.get_profile_order(device)?;
		Ok(sort_profiles(get_device_profiles(device)?, &order))
	}

	pub fn get_selected_profile(&mut self, device: &str) -> Result<String, anyhow::Error> {
		self.config_store(device)?;

		let from_store = &self.stores.get(device).unwrap().value.selected_profile.clone();
		let all = self.get_ordered_profiles(device)?;
		if all.contains(from_store) { Ok(from_store.clone()) } else { Ok(all.first().unwrap().clone()) }
	}

	pub fn set_selected_profile(&mut self, device: &str, id: String) -> Result<(), anyhow::Error> {
		let store = self.config_store(device)?;
		store.value.selected_profile = id;
		store.save()?;
		Ok(())
	}
}

/// Sort profile IDs by a user-defined order. IDs missing from the order keep alphabetical order after the listed ones.
pub fn sort_profiles(mut profiles: Vec<String>, order: &[String]) -> Vec<String> {
	profiles.sort();
	profiles.dedup();
	profiles.sort_by_key(|id| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
	profiles
}

pub fn get_device_profiles(device: &str) -> Result<Vec<String>, anyhow::Error> {
	let mut profiles: Vec<String> = vec![];

	let device_path = config_dir().join("profiles").join(device);
	fs::create_dir_all(&device_path)?;
	let entries = fs::read_dir(device_path)?;

	for entry in entries.flatten() {
		if entry.path().is_file() {
			let mut id = entry.file_name().to_string_lossy().into_owned();
			if id.ends_with(".json") {
				id.truncate(id.len() - 5);
			} else if id.ends_with(".json.bak") {
				id.truncate(id.len() - 9);
			} else if id.ends_with(".json.temp") {
				id.truncate(id.len() - 10);
			} else {
				continue;
			}
			profiles.push(id);
		} else if entry.path().is_dir() {
			let entries = fs::read_dir(entry.path())?;
			for subentry in entries.flatten() {
				if subentry.path().is_file() {
					let mut id = format!("{}/{}", entry.file_name().to_string_lossy(), subentry.file_name().to_string_lossy());
					if id.ends_with(".json") {
						id.truncate(id.len() - 5);
					} else if id.ends_with(".json.bak") {
						id.truncate(id.len() - 9);
					} else if id.ends_with(".json.temp") {
						id.truncate(id.len() - 10);
					} else {
						continue;
					}
					profiles.push(id);
				}
			}
		}
	}

	if profiles.is_empty() {
		profiles.push("Default".to_owned());
	}

	Ok(profiles)
}

/// A singleton object to contain all active Store instances that hold a profile.
pub static PROFILE_STORES: LazyLock<RwLock<ProfileStores>> = LazyLock::new(|| RwLock::new(ProfileStores { stores: HashMap::new() }));

/// A singleton object to manage Store instances for device configurations.
pub static DEVICE_STORES: LazyLock<RwLock<DeviceStores>> = LazyLock::new(|| RwLock::new(DeviceStores { stores: HashMap::new() }));

pub struct Locks<'a> {
	#[allow(dead_code)]
	pub device_stores: RwLockReadGuard<'a, DeviceStores>,
	pub profile_stores: RwLockReadGuard<'a, ProfileStores>,
}

pub async fn acquire_locks() -> Locks<'static> {
	let device_stores = DEVICE_STORES.read().await;
	let profile_stores = PROFILE_STORES.read().await;
	Locks { device_stores, profile_stores }
}

pub struct LocksMut<'a> {
	pub device_stores: RwLockWriteGuard<'a, DeviceStores>,
	pub profile_stores: RwLockWriteGuard<'a, ProfileStores>,
}

pub async fn acquire_locks_mut() -> LocksMut<'static> {
	let device_stores = DEVICE_STORES.write().await;
	let profile_stores = PROFILE_STORES.write().await;
	LocksMut { device_stores, profile_stores }
}

pub async fn get_slot<'a>(context: &crate::shared::Context, locks: &'a Locks<'_>) -> Result<&'a Option<crate::shared::ActionInstance>, anyhow::Error> {
	let device = DEVICES.get(&context.device).ok_or_else(|| anyhow!("device not found"))?;
	let store = locks.profile_stores.get_profile_store(&device, &context.profile)?;

	let configured = match &context.controller[..] {
		"Encoder" => store.value.sliders.get(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
		"Infobar" => store.value.infobars.get(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
		_ => store.value.keys.get(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
	};

	Ok(configured)
}

pub async fn get_slot_mut<'a>(context: &crate::shared::Context, locks: &'a mut LocksMut<'_>) -> Result<&'a mut Option<crate::shared::ActionInstance>, anyhow::Error> {
	let device = DEVICES.get(&context.device).ok_or_else(|| anyhow!("device not found"))?;
	let store = locks.profile_stores.get_profile_store_mut(&device, &context.profile).await?;

	let configured = match &context.controller[..] {
		"Encoder" => store.value.sliders.get_mut(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
		"Infobar" => store.value.infobars.get_mut(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
		_ => store.value.keys.get_mut(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
	};

	Ok(configured)
}

pub async fn get_instance<'a>(context: &crate::shared::ActionContext, locks: &'a Locks<'_>) -> Result<Option<&'a crate::shared::ActionInstance>, anyhow::Error> {
	let slot = get_slot(&(context.into()), locks).await?;
	if let Some(instance) = slot {
		if instance.context == *context {
			return Ok(Some(instance));
		} else if let Some(children) = &instance.children {
			for child in children {
				if child.context == *context {
					return Ok(Some(child));
				}
			}
		}
	}
	Ok(None)
}

pub async fn get_instance_mut<'a>(context: &crate::shared::ActionContext, locks: &'a mut LocksMut<'_>) -> Result<Option<&'a mut crate::shared::ActionInstance>, anyhow::Error> {
	let slot = get_slot_mut(&(context.into()), locks).await?;
	if let Some(instance) = slot {
		if instance.context == *context {
			return Ok(Some(instance));
		} else if let Some(children) = &mut instance.children {
			for child in children {
				if child.context == *context {
					return Ok(Some(child));
				}
			}
		}
	}
	Ok(None)
}

pub async fn mark_profile_stale(device_id: &str, locks: &mut LocksMut<'_>) -> Result<(), anyhow::Error> {
	let selected_profile = locks.device_stores.get_selected_profile(device_id)?;
	let device = DEVICES.get(device_id).ok_or_else(|| anyhow!("device not found"))?;
	let store = locks.profile_stores.get_profile_store_mut(&device, &selected_profile).await?;
	store.value.stale = true;
	Ok(())
}

pub async fn save_profile_now(device_id: &str, locks: &mut LocksMut<'_>) -> Result<(), anyhow::Error> {
	let selected_profile = locks.device_stores.get_selected_profile(device_id)?;
	let device = DEVICES.get(device_id).ok_or_else(|| anyhow!("device not found"))?;
	let store = locks.profile_stores.get_profile_store_mut(&device, &selected_profile).await?;

	store.save()?;
	store.value.stale = false;

	Ok(())
}

pub async fn flush_stale_profiles() -> Result<(), anyhow::Error> {
	let mut locks = acquire_locks_mut().await;
	for store in locks.profile_stores.stores.values_mut() {
		if store.value.stale {
			store.save()?;
			store.value.stale = false;
		}
	}
	Ok(())
}

/// Identify a slot on a device independently of the profile, e.g. `Keypad.3`.
pub fn slot_key(controller: &str, position: u8) -> String {
	format!("{controller}.{position}")
}

fn profile_slot_mut<'a>(profile: &'a mut Profile, key: &str) -> Option<&'a mut Option<ActionInstance>> {
	let (controller, position) = key.split_once('.')?;
	let position: usize = position.parse().ok()?;
	match controller {
		"Encoder" => profile.sliders.get_mut(position),
		"Infobar" => profile.infobars.get_mut(position),
		"Keypad" => profile.keys.get_mut(position),
		_ => None,
	}
}

fn instance_images_dir(context: &crate::shared::ActionContext) -> PathBuf {
	config_dir()
		.join("images")
		.join(&context.device)
		.join(&context.profile)
		.join(format!("{}.{}.{}", context.controller, context.position, context.index))
}

/// Make a copy of an instance for another profile, pointing any per-instance images at that profile's image folder.
/// Returns the copy and the list of image folders to copy across, as (from, to).
fn retarget_instance(instance: &ActionInstance, profile: &str) -> (ActionInstance, Vec<(PathBuf, PathBuf)>) {
	let mut copy = instance.clone();
	let mut dirs = vec![];
	let mut fix = |instance: &mut ActionInstance| {
		let old_dir = instance_images_dir(&instance.context);
		instance.context.profile = profile.to_owned();
		let new_dir = instance_images_dir(&instance.context);
		let rewrite = |image: &mut String| {
			let path = std::path::Path::new(image.as_str());
			if path.starts_with(&old_dir) {
				*image = new_dir.join(path.strip_prefix(&old_dir).unwrap()).to_string_lossy().into_owned();
			}
		};
		for state in instance.states.iter_mut() {
			rewrite(&mut state.image);
			if let Some(image_override) = state.image_override.as_mut() {
				rewrite(image_override);
			}
		}
		dirs.push((old_dir, new_dir));
	};
	fix(&mut copy);
	if let Some(children) = copy.children.as_mut() {
		for child in children.iter_mut() {
			fix(child);
		}
	}
	(copy, dirs)
}

fn instance_dirs(instance: &ActionInstance) -> Vec<PathBuf> {
	let mut dirs = vec![instance_images_dir(&instance.context)];
	if let Some(children) = &instance.children {
		dirs.extend(children.iter().map(|c| instance_images_dir(&c.context)));
	}
	dirs
}

/// Copy the contents of every replicated slot of `source` onto every other profile of the device.
/// A replicated slot that is empty on `source` is cleared everywhere and stops being replicated.
pub async fn propagate_replicated(device_id: &str, source: &str, locks: &mut LocksMut<'_>) -> Result<(), anyhow::Error> {
	let keys = locks.device_stores.get_replicated(device_id)?;
	if keys.is_empty() {
		return Ok(());
	}
	let device = DEVICES.get(device_id).ok_or_else(|| anyhow!("device not found"))?.clone();

	let source_store = locks.profile_stores.get_profile_store_mut(&device, source).await?;
	let mut slots: Vec<(String, Option<ActionInstance>)> = vec![];
	for key in &keys {
		if let Some(slot) = profile_slot_mut(&mut source_store.value, key) {
			slots.push((key.clone(), slot.clone()));
		}
	}

	for profile in get_device_profiles(device_id)? {
		if profile == source {
			continue;
		}
		let store = locks.profile_stores.get_profile_store_mut(&device, &profile).await?;
		let mut changed = false;
		for (key, instance) in &slots {
			let Some(slot) = profile_slot_mut(&mut store.value, key) else { continue };
			let mut new = instance.as_ref().map(|i| retarget_instance(i, &profile));
			// Titles and images set by the plugin at runtime (as opposed to the user's overrides) belong to each copy:
			// a plugin may show something different on each profile, e.g. the name of the next page.
			if let (Some(old), Some((new, _))) = (&*slot, &mut new)
				&& old.action.uuid == new.action.uuid
			{
				for (new_state, old_state) in new.states.iter_mut().zip(&old.states) {
					new_state.text.clone_from(&old_state.text);
					new_state.image.clone_from(&old_state.image);
				}
			}
			let same = match (&*slot, &new) {
				(None, None) => true,
				(Some(old), Some((new, _))) => serde_json::to_value(old).ok() == serde_json::to_value(new).ok(),
				_ => false,
			};
			if same {
				continue;
			}
			if let Some(old) = slot.as_ref() {
				for dir in instance_dirs(old) {
					let _ = fs::remove_dir_all(dir);
				}
			}
			if let Some((_, dirs)) = &new {
				for (from, to) in dirs {
					if from.is_dir() {
						let _ = copy_dir(from, to);
					}
				}
			}
			*slot = new.map(|(i, _)| i);
			changed = true;
		}
		if changed {
			store.save()?;
			store.value.stale = false;
		}
	}

	let emptied: Vec<&String> = slots.iter().filter(|(_, i)| i.is_none()).map(|(k, _)| k).collect();
	if !emptied.is_empty() {
		let remaining = keys.iter().filter(|k| !emptied.contains(k)).cloned().collect();
		locks.device_stores.set_replicated(device_id, remaining)?;
	}

	Ok(())
}
