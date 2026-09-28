import assert from "node:assert/strict";
import { test } from "node:test";
import { groupMeasurementDrafts } from "../src/measurementGroups.ts";

const first={
  measurements:{"طول أمام":"55"},fabric:{"اسم القماش":"أبيض"},fabricItemId:null,
  fabricMeters:"3.5",designs:{"نوع الثوب":{id:1}},thobeType:"سعودي",
  sleeveMode:"سادة",collarMode:"قلاب",neckButtonCount:"1",neckButtonType:"طقطق",
  tailorName:"محمد",sizeCategory:"كبير",notes:"",workerName:"أحمد",tailorPrice:"23"
};

test("two identical printable thobes plus a different third produce two sheets",()=>{
  const second={...first,workerName:"علي",tailorPrice:"25"};
  const third={...first,measurements:{"طول أمام":"60"}};
  const groups=groupMeasurementDrafts([first,second,third]);
  assert.deepEqual(groups.map(group=>group.indices),[[0,1],[2]]);
  assert.equal(groups[0].thobe,first);
});

test("matching thobes stay together even if they are not adjacent",()=>{
  const different={...first,sleeveMode:"كبك"};
  assert.deepEqual(groupMeasurementDrafts([first,different,{...first}]).map(group=>group.indices),[[0,2],[1]]);
});

test("one different value shown on the sheet creates a separate sheet",()=>{
  assert.equal(groupMeasurementDrafts([first,{...first,tailorName:"علي"}]).length,2);
  assert.equal(groupMeasurementDrafts([first,{...first,designs:{"نوع الثوب":{id:2}}}]).length,2);
  assert.equal(groupMeasurementDrafts([first,{...first,fabricUnit:"ياردة"}]).length,2);
});

test("field insertion order does not create an extra page",()=>{
  const firstWithTwoFields={...first,measurements:{"طول أمام":"55","طول خلف":"56"}};
  const sameWithReverseOrder={...first,measurements:{"طول خلف":"56","طول أمام":"55"}};
  assert.deepEqual(groupMeasurementDrafts([firstWithTwoFields,sameWithReverseOrder]).map(group=>group.indices),[[0,1]]);
});

test("an older invoice without a unit still groups with meter invoices",()=>{
  assert.deepEqual(groupMeasurementDrafts([first,{...first,fabricUnit:"متر"}]).map(group=>group.indices),[[0,1]]);
});
