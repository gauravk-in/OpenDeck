use super::Error;

use crate::shared::DEVICES;
use crate::store::profiles::{acquire_locks_mut, propagate_replicated, save_profile_now, slot_key};

use tauri::{AppHandle, Emitter, Manager, command};

#[command]
pub async fn get_profiles(device: &str) -> Result<Vec<String>, Error> {
	let mut device_stores = crate::store::profiles::DEVICE_STORES.write().await;
	Ok(device_stores.get_ordered_profiles(device)?)
}

#[command]
pub async fn set_profile_order(device: String, order: Vec<String>) -> Result<(), Error> {
	let mut device_stores = crate::store::profiles::DEVICE_STORES.write().await;
	device_stores.set_profile_order(&device, order)?;
	Ok(())
}

#[command]
pub async fn get_replicated_slots(device: String) -> Result<Vec<String>, Error> {
	let mut device_stores = crate::store::profiles::DEVICE_STORES.write().await;
	Ok(device_stores.get_replicated(&device)?)
}

/// Mark a slot as replicated, copying its current action from `context.profile` onto every other profile of the device,
/// or stop replicating it, leaving the copies on each profile as independent actions.
#[command]
pub async fn set_slot_replicated(app: AppHandle, context: crate::shared::Context, replicated: bool) -> Result<Vec<String>, Error> {
	let mut locks = acquire_locks_mut().await;
	if !DEVICES.contains_key(&context.device) {
		return Err(Error::new(format!("device {} not found", context.device)));
	}

	let key = slot_key(&context.controller, context.position);
	let mut slots = locks.device_stores.get_replicated(&context.device)?;
	let empty = crate::store::profiles::get_slot_mut(&context, &mut locks).await?.is_none();
	if replicated {
		if empty {
			return Err(Error::new("only a slot with an action can be replicated".to_owned()));
		}
		if !slots.contains(&key) {
			slots.push(key.clone());
		}
		save_profile_now(&context.device, &mut locks).await?;
		locks.device_stores.set_replicated(&context.device, slots)?;
		propagate_replicated(&context.device, &context.profile, &mut locks).await?;
	} else if empty && slots.contains(&key) {
		// The replicated action was deleted: delete it from every profile, which also stops replicating the slot.
		propagate_replicated(&context.device, &context.profile, &mut locks).await?;
	} else {
		// Stop replicating but leave each profile's copy in place as an independent action.
		slots.retain(|k| *k != key);
		locks.device_stores.set_replicated(&context.device, slots)?;
	}

	let slots = locks.device_stores.get_replicated(&context.device)?;
	let _ = app.get_webview_window("main").unwrap().emit("replicated_slots", (&context.device, &slots));
	Ok(slots)
}

#[command]
pub async fn get_selected_profile(device: String) -> Result<crate::shared::Profile, Error> {
	let mut locks = acquire_locks_mut().await;
	if !DEVICES.contains_key(&device) {
		return Err(Error::new(format!("device {device} not found")));
	}

	let selected_profile = locks.device_stores.get_selected_profile(&device)?;
	let profile = locks.profile_stores.get_profile_store(&DEVICES.get(&device).unwrap(), &selected_profile)?;

	Ok(profile.value.clone())
}

#[allow(clippy::flat_map_identity)]
#[command]
pub async fn set_selected_profile(device: String, id: String) -> Result<(), Error> {
	let mut locks = acquire_locks_mut().await;
	if !DEVICES.contains_key(&device) {
		return Err(Error::new(format!("device {device} not found")));
	}

	// If a profile save is pending for this device, save it immediately to prevent losing profile data
	if let Err(error) = save_profile_now(&device, &mut locks).await {
		log::error!("Failed to save profile for device {device}: {error}");
	}

	let selected_profile = locks.device_stores.get_selected_profile(&device)?;

	if selected_profile != id {
		let old_profile = &locks.profile_stores.get_profile_store(&DEVICES.get(&device).unwrap(), &selected_profile)?.value;
		for instance in old_profile
			.keys
			.iter()
			.flatten()
			.chain(&mut old_profile.sliders.iter().flatten())
			.chain(&mut old_profile.infobars.iter().flatten())
		{
			if !matches!(instance.action.uuid.as_str(), "opendeck.multiaction" | "opendeck.toggleaction") {
				let _ = crate::events::outbound::will_appear::will_disappear(instance, false).await;
			} else {
				for child in instance.children.as_ref().unwrap() {
					let _ = crate::events::outbound::will_appear::will_disappear(child, false).await;
				}
			}
		}
		let _ = crate::events::outbound::devices::clear_screen(device.clone()).await;

		// Make sure the new profile exists, then bring the replicated slots of every profile in line with the one being left.
		locks.profile_stores.get_profile_store_mut(&DEVICES.get(&device).unwrap(), &id).await?;
		if let Err(error) = propagate_replicated(&device, &selected_profile, &mut locks).await {
			log::error!("Failed to update replicated slots for device {device}: {error}");
		}
	}

	// We must use the mutable version of get_profile_store in order to create the store if it does not exist.
	let store = locks.profile_stores.get_profile_store_mut(&DEVICES.get(&device).unwrap(), &id).await?;
	let new_profile = &store.value;
	for instance in new_profile
		.keys
		.iter()
		.flatten()
		.chain(&mut new_profile.sliders.iter().flatten())
		.chain(&mut new_profile.infobars.iter().flatten())
	{
		if !matches!(instance.action.uuid.as_str(), "opendeck.multiaction" | "opendeck.toggleaction") {
			let _ = crate::events::outbound::will_appear::will_appear(instance).await;
		} else {
			for child in instance.children.as_ref().unwrap() {
				let _ = crate::events::outbound::will_appear::will_appear(child).await;
			}
		}
	}
	store.save()?;

	locks.device_stores.set_selected_profile(&device, id)?;

	Ok(())
}

#[command]
pub async fn delete_profile(device: String, profile: String) {
	let mut locks = acquire_locks_mut().await;
	locks.profile_stores.delete_profile(&device, &profile);
	if let Ok(mut order) = locks.device_stores.get_profile_order(&device)
		&& order.contains(&profile)
	{
		order.retain(|p| *p != profile);
		let _ = locks.device_stores.set_profile_order(&device, order);
	}
}

#[command]
pub async fn rename_profile(device: String, old_id: String, new_id: String, retain: bool) -> Result<(), Error> {
	let mut locks = acquire_locks_mut().await;
	if !DEVICES.contains_key(&device) {
		return Err(Error::new(format!("device {device} not found")));
	}

	let mut order = locks.device_stores.get_ordered_profiles(&device)?;
	locks.profile_stores.rename_profile(&DEVICES.get(&device).unwrap(), &old_id, &new_id, retain).await?;

	// Keep the custom order: a renamed profile keeps its place, a duplicate goes right after its original.
	order.retain(|p| *p != new_id);
	match order.iter().position(|p| *p == old_id) {
		Some(index) if retain => order.insert(index + 1, new_id),
		Some(index) => order[index] = new_id,
		None => order.push(new_id),
	}
	locks.device_stores.set_profile_order(&device, order)?;

	Ok(())
}

pub async fn rerender_images(app: &AppHandle) -> Result<(), anyhow::Error> {
	let window = app.get_webview_window("main").unwrap();
	window.emit("rerender_images", ())?;
	Ok(())
}
