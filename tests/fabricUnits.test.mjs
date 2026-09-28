import assert from "node:assert/strict";
import { test } from "node:test";
import { changeFabricLengthUnit, fromMeters, toMeters } from "../src/fabricUnits.ts";

test("yard stock is stored as meters using the exact yard definition",()=>{
  assert.equal(toMeters(100,"ياردة"),91.44);
  assert.equal(toMeters(100,"متر"),100);
  assert.equal(fromMeters(91.44,"ياردة"),100);
});

test("changing order units preserves quantity and empty input",()=>{
  const yards=changeFabricLengthUnit("3.5","متر","ياردة");
  assert.ok(Math.abs(toMeters(Number(yards),"ياردة")-3.5)<0.000001);
  assert.equal(changeFabricLengthUnit("","متر","ياردة"),"");
});
