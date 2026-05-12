class FakeMap {
	on() {}
	off() {}
	once(_event: string, cb: () => void) {
		cb();
	}
	remove() {}
	getCanvas() {
		return document.createElement('canvas') as HTMLCanvasElement;
	}
	getSource() {
		return undefined;
	}
	setStyle() {}
	project() {
		return { x: 0, y: 0 };
	}
	jumpTo() {}
	easeTo() {}
	fitBounds() {}
	addControl() {}
	removeControl() {}
	hasImage() {
		return false;
	}
	addImage() {}
}
class FakeMarker {
	setLngLat() {
		return this;
	}
	addTo() {
		return this;
	}
	remove() {}
}
class FakeLngLatBounds {
	extend() {
		return this;
	}
}
const def = {
	Map: FakeMap,
	Marker: FakeMarker,
	LngLatBounds: FakeLngLatBounds,
};
export {
	def as default,
	FakeMap as Map,
	FakeMarker as Marker,
	FakeLngLatBounds as LngLatBounds,
};
