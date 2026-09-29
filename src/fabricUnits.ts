export type FabricLengthUnit = "متر" | "ياردة";
export const DEFAULT_ORDER_FABRIC_UNIT: FabricLengthUnit = "ياردة";

export const METERS_PER_YARD = 0.9144;

export function toMeters(quantity: number, unit: FabricLengthUnit): number {
  return unit === "ياردة" ? quantity * METERS_PER_YARD : quantity;
}

export function fromMeters(meters: number, unit: FabricLengthUnit): number {
  return unit === "ياردة" ? meters / METERS_PER_YARD : meters;
}

export function changeFabricLengthUnit(value: string, current: FabricLengthUnit, next: FabricLengthUnit): string {
  if (!value.trim() || current === next) return value;
  const quantity = Number(value);
  return Number.isFinite(quantity) ? String(Number(fromMeters(toMeters(quantity, current), next).toFixed(6))) : value;
}
