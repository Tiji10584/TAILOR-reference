import assert from "node:assert/strict";
import { test } from "node:test";
import { isFinalThobeSelected } from "../src/orderPrintRules.ts";

test("printing remains available for a single thobe",()=>{
  assert.equal(isFinalThobeSelected(0,1),true);
});

test("printing from earlier garments must wait for the final one",()=>{
  assert.equal(isFinalThobeSelected(0,2),false);
  assert.equal(isFinalThobeSelected(1,2),true);
  assert.equal(isFinalThobeSelected(0,3),false);
  assert.equal(isFinalThobeSelected(1,3),false);
  assert.equal(isFinalThobeSelected(2,3),true);
});
