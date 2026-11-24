/// Position of one controller in a custom device layout, in units of one key slot.
export type LayoutPosition = {
	x: number;
	y: number;
	/// Display size relative to a normal key slot; defaults to 1.
	scale?: number;
};

/// Maps a controller key such as "Keypad.3" or "Encoder.0" to its position.
export type DeviceLayout = { [controller: string]: LayoutPosition };
