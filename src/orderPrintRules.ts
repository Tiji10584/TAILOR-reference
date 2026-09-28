export function isFinalThobeSelected(currentIndex:number,totalThobes:number):boolean{
  return totalThobes<=1||currentIndex===totalThobes-1;
}
