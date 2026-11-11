import assert from 'node:assert/strict';
import test from 'node:test';
import { textNumber } from '../../frontend/texting/numbers.js';

test('direct number entry accepts the phone API forms and keeps equivalent numbers together', () => {
  for (const [raw, number] of [
    ['(206) 555-0148', '+12065550148'],
    ['206.555.0148', '+12065550148'],
    ['+1 206 555 0148', '+12065550148'],
    ['12065550148', '+12065550148'],
    ['+44 20 7946 0958', '+442079460958'],
    ['72166', '72166'],
    ['', undefined],
    ['call me', undefined],
    ['555-01', undefined],
    ['20655501', undefined],
    ['+1234567890123456', undefined],
    ['(206) 555-0148 extension 1', undefined],
    ['206' + ' '.repeat(30) + '5550148', undefined],
  ] as const)
    assert.equal(textNumber(raw), number, raw);
});
