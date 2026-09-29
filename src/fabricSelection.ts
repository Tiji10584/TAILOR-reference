type FabricChoice = { fabricItemId: number | null; fabric: Record<string, string> };

export function firstThobeWithoutFabric(thobes: FabricChoice[]): number {
  return thobes.findIndex(thobe => {
    const name = thobe.fabric["اسم القماش"]?.trim();
    return !(thobe.fabricItemId !== null && thobe.fabricItemId > 0 && name) && name !== "قماش العميل";
  });
}
