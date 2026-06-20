export function hotspotActionLabel(hotspot) {
    const label = String(hotspot?.label || 'Hotspot');
    return hotspot?.action ? `${label} (${hotspot.action})` : label;
}

export function npcActionLabel(npc) {
    const label = String(npc?.label || 'Someone');
    return npc?.slotId ? `${label} at ${npc.slotId}` : label;
}
