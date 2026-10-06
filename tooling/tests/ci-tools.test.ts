import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { assessmentFixture, namedFixture } from '../testing/ci-tools.js';

test('the browser suite uses a restored assessment fixture only from its own CI cache', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-ci-tools-'));
  try {
    const tools = path.join(root, '.ci-tools');
    const env = { CI: 'true', DISPATCH_CI_TOOLS: tools };
    assert.equal(assessmentFixture(env, root), undefined, 'nothing restored yet');

    const binary = path.join(tools, 'fixture/assessment-fixture');
    fs.mkdirSync(path.dirname(binary), { recursive: true });
    fs.writeFileSync(binary, '#!/bin/sh\n', { mode: 0o600 });
    assert.equal(assessmentFixture(env, root), undefined, 'a non-executable file is not a tool');
    fs.chmodSync(binary, 0o700);
    assert.equal(assessmentFixture(env, root), binary);
    assert.equal(assessmentFixture({ ...env, DISPATCH_CI_TOOLS: `${tools}/` }, root), binary);

    assert.equal(assessmentFixture({ DISPATCH_CI_TOOLS: tools }, root), undefined, 'CI only');
    assert.equal(assessmentFixture({ CI: 'true' }, root), undefined, 'explicit location only');
    const elsewhere = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-ci-tools-other-'));
    try {
      fs.mkdirSync(path.join(elsewhere, 'fixture'));
      fs.copyFileSync(binary, path.join(elsewhere, 'fixture/assessment-fixture'));
      assert.equal(
        assessmentFixture({ CI: 'true', DISPATCH_CI_TOOLS: elsewhere }, root),
        undefined,
        'only the workspace cache directory is trusted',
      );
    } finally {
      fs.rmSync(elsewhere, { recursive: true, force: true });
    }

    fs.rmSync(binary);
    fs.symlinkSync('/bin/sh', binary);
    assert.equal(assessmentFixture(env, root), undefined, 'a symlink is not a tool');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('off CI the browser suite uses the assessment fixture dispatchdev names', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-named-fixture-'));
  try {
    const binary = path.join(root, 'assessment-fixture');
    const env = { DISPATCH_ASSESSMENT_FIXTURE: binary };
    assert.equal(namedFixture(env), undefined, 'nothing built yet');
    fs.writeFileSync(binary, '#!/bin/sh\n', { mode: 0o600 });
    assert.equal(namedFixture(env), undefined, 'a non-executable file is not a tool');
    fs.chmodSync(binary, 0o700);
    assert.equal(namedFixture(env), binary);
    assert.equal(namedFixture({}), undefined, 'only when named');
    assert.equal(namedFixture({ ...env, CI: 'true' }), undefined, 'CI trusts only its own cache');
    fs.rmSync(binary);
    fs.symlinkSync('/bin/sh', binary);
    assert.equal(namedFixture(env), undefined, 'a symlink is not a tool');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
