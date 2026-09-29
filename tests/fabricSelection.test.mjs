import test from 'node:test';
import assert from 'node:assert/strict';
import { firstThobeWithoutFabric } from '../src/fabricSelection.ts';

const selected = (id, name) => ({ fabricItemId: id, fabric: { 'اسم القماش': name } });

test('every thobe needs an explicit stock or customer fabric before saving and printing', () => {
  assert.equal(firstThobeWithoutFabric([selected(1, 'قطن — أبيض'), selected(null, 'قماش العميل')]), -1);
  assert.equal(firstThobeWithoutFabric([selected(1, 'قطن — أبيض'), selected(null, '')]), 1);
  assert.equal(firstThobeWithoutFabric([selected(null, 'قطن — أبيض')]), 0);
  assert.equal(firstThobeWithoutFabric([selected(1, '  ')]), 0);
});
