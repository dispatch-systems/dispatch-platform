import assert from 'node:assert/strict';
import test from 'node:test';
import { driveLinks } from '../../frontend/driveLinks.js';

test('a pasted Google link opens the picker on the file or folder it names', () => {
  const cases: [string, ReturnType<typeof driveLinks>][] = [
    ['https://docs.google.com/spreadsheets/d/1Ab-c_9/edit#gid=0', { files: ['1Ab-c_9'] }],
    ['https://docs.google.com/document/u/1/d/1Ab-c_9/edit?usp=sharing', { files: ['1Ab-c_9'] }],
    ['https://drive.google.com/file/d/1Ab-c_9/view?usp=drive_link', { files: ['1Ab-c_9'] }],
    ['https://drive.google.com/open?id=1Ab-c_9', { files: ['1Ab-c_9'] }],
    ['https://drive.google.com/drive/u/0/folders/1Fo-l_d', { files: [], folder: '1Fo-l_d' }],
    // Several links: each file once, and files before any folder.
    [
      ' https://drive.google.com/drive/folders/1Fo-l_d\nhttps://docs.google.com/presentation/d/1Sl/edit https://docs.google.com/document/d/1Do/edit\nhttps://docs.google.com/presentation/d/1Sl/edit ',
      { files: ['1Sl', '1Do'] },
    ],
    // A published copy's address names no file, and other text and sites name none.
    ['https://docs.google.com/spreadsheets/d/e/2PACX-1vQ/pubhtml', undefined],
    ['http://docs.google.com/document/d/1Ab/edit', undefined],
    ['https://docs.google.com.example.net/document/d/1Ab/edit', undefined],
    ['https://docs.google.com/document/d/1Ab%3Cb%3E/edit', undefined],
    ['Fuel receipts for October', undefined],
    ['', undefined],
  ];
  for (const [pasted, at] of cases) assert.deepEqual(driveLinks(pasted), at, pasted);
});
