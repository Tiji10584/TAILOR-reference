import assert from "node:assert/strict";
import { test } from "node:test";
import { changeFabricLengthUnit, DEFAULT_ORDER_FABRIC_UNIT, fromMeters, toMeters } from "../src/fabricUnits.ts";

test("new orders default to yards without changing inventory conversion",()=>{
  assert.equal(DEFAULT_ORDER_FABRIC_UNIT,"ياردة");
  assert.equal(toMeters(3,DEFAULT_ORDER_FABRIC_UNIT),2.7432);
});

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
