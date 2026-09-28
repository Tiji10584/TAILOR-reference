export type PrintableThobe={
  measurements:Record<string,string>;
  fabric:Record<string,string>;
  fabricItemId:number|null;
  fabricMeters:string;
  designs:Record<string,{id:number}|undefined>;
  thobeType:string;
  sleeveMode:string;
  collarMode:string;
  neckButtonCount:string;
  neckButtonType:string;
  tailorName:string;
  sizeCategory:string;
  notes:string;
};

// Only fields visible on the worker's measurement sheet determine whether sheets match.
export function measurementSheetKey(thobe:PrintableThobe):string{
  const ordered=(values:Record<string,string>)=>Object.fromEntries(Object.entries(values).sort(([a],[b])=>a.localeCompare(b)));
  const designs=Object.fromEntries(Object.entries(thobe.designs)
    .filter((entry):entry is [string,{id:number}]=>Boolean(entry[1]))
    .sort(([a],[b])=>a.localeCompare(b))
    .map(([category,option])=>[category,option.id]));
  return JSON.stringify({
    measurements:ordered(thobe.measurements),fabric:ordered(thobe.fabric),fabricItemId:thobe.fabricItemId,
    fabricMeters:thobe.fabricMeters,designs,thobeType:thobe.thobeType,
    sleeveMode:thobe.sleeveMode,collarMode:thobe.collarMode,
    neckButtonCount:thobe.neckButtonCount,neckButtonType:thobe.neckButtonType,
    tailorName:thobe.tailorName,sizeCategory:thobe.sizeCategory,notes:thobe.notes
  });
}

export function groupMeasurementDrafts<T extends PrintableThobe>(drafts:readonly T[]):{thobe:T;indices:number[]}[]{
  const groups:{thobe:T;indices:number[]}[]=[];
  const byKey=new Map<string,number>();
  drafts.forEach((thobe,index)=>{
    const key=measurementSheetKey(thobe),existing=byKey.get(key);
    if(existing===undefined){byKey.set(key,groups.length);groups.push({thobe,indices:[index]})}
    else groups[existing].indices.push(index);
  });
  return groups;
}
