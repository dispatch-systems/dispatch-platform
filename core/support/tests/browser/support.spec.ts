import { test, expect, demo, login } from '../../../shell/tests/support/fixtures.js';

const screenshot = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j6mkAAAAASUVORK5CYII=',
  'base64',
);

test('a member reports with a screenshot, follows the ticket and the owner approves its plan', async ({
  page,
  dispatch,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.setViewportSize({ width: 390, height: 844 });
  await login(page, demo.member);
  await page.getByRole('button', { name: 'Open navigation' }).click();
  await page.getByLabel('Account menu', { exact: true }).click();
  await page.getByRole('button', { name: 'AI Support', exact: true }).click();
  const panel = page.getByRole('dialog', { name: 'AI Support', exact: true });
  await expect(panel).toBeVisible();
  await expect(page.getByRole('button', { name: 'Close navigation' })).toBeHidden();
  await panel.getByLabel('Title', { exact: true }).fill('An employee disappears after refreshing');
  await panel
    .getByLabel('Description', { exact: true })
    .fill(
      'After opening an employee and refreshing, the selected employee disappears. I expected the selection to stay.',
    );
  await panel
    .getByLabel('Upload screenshots')
    .setInputFiles({ name: 'employee.png', mimeType: 'image/png', buffer: screenshot });
  await expect(panel.getByRole('img', { name: 'employee.png' })).toBeVisible();
  await panel.getByRole('button', { name: 'Send report', exact: true }).click();
  await expect(
    panel.getByRole('heading', { name: 'An employee disappears after refreshing', exact: true }),
  ).toBeVisible();
  await expect(panel.getByText('Reviewing message', { exact: true })).toBeVisible();
  await expect(panel.getByText('Routing your request', { exact: true })).toBeVisible();
  await expect(panel.getByRole('img', { name: 'employee.png' })).toBeVisible();
  await panel
    .getByLabel('Reply', { exact: true })
    .fill('This also happens when I reopen the browser.');
  await panel.getByRole('button', { name: 'Send reply', exact: true }).click();
  await expect(panel.getByRole('list', { name: 'Ticket conversation' })).toContainText(
    'This also happens when I reopen the browser.',
  );
  await page.reload();
  await page.getByRole('button', { name: 'Open navigation' }).click();
  await page.getByLabel('Account menu', { exact: true }).click();
  await page.getByRole('button', { name: 'AI Support', exact: true }).click();
  await panel.getByRole('tab', { name: 'My tickets', exact: true }).click();
  await panel.getByRole('button', { name: /An employee disappears after refreshing/ }).click();
  await expect(panel.getByRole('list', { name: 'Ticket conversation' })).toContainText(
    'This also happens when I reopen the browser.',
  );
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await panel.getByRole('button', { name: 'Close AI Support' }).click();

  // Arrange an agent proposal to exercise the owner's real browser decision path. The worker
  // protocol and approval authority have their main tests at the backend/API layer.
  let ticketId = '';
  dispatch.database('data/platform/accounts.sqlite', (db) => {
    const ticket = db
      .prepare('SELECT id FROM support_tickets WHERE title=?')
      .get('An employee disappears after refreshing')!;
    ticketId = String(ticket.id);
    const proposal = {
      version: 1,
      sourceSha: 'a'.repeat(40),
      reproduction: 'Selection was lost after a browser refresh.',
      observations: ['The selected employee is absent after refresh.'],
      uncertainties: [],
      analysis: 'The selected employee is only held in component state.',
      plan: ['Keep the employee identifier in the existing page address.'],
      alternatives: ['Save the selection in session memory.'],
      acceptanceCriteria: ['The selected employee remains visible after refresh.'],
      verification: ['Select an employee, refresh and confirm the employee.'],
      productionVerification: [
        'Open the employee directory and confirm the selection controls are visible without saving or collecting.',
      ],
      affectedAreas: ['Employee navigation'],
      risks: ['A stale identifier must recover to the directory.'],
      createdAt: Date.now(),
    };
    db.prepare(
      'INSERT INTO support_proposals(ticket_id,version,body,created_at) VALUES(?,?,?,?)',
    ).run(ticketId, 1, JSON.stringify(proposal), Date.now());
    db.prepare(
      "UPDATE support_tickets SET status='awaiting_approval',proposal_version=1,progress='Ready for owner review' WHERE id=?",
    ).run(ticketId);
  });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await login(page);
  await page.getByRole('link', { name: 'Support', exact: true }).click();
  await page.getByRole('button', { name: /An employee disappears after refreshing/ }).click();
  const details = page.getByRole('region', { name: 'Ticket details' });
  await expect(details).toContainText('Northline Logistics');
  await expect(details).toContainText('The selected employee remains visible after refresh.');
  await details.getByRole('button', { name: 'Ask a question', exact: true }).click();
  await expect(details.getByRole('combobox', { name: 'Send to', exact: true })).toHaveValue(
    'agent',
  );
  await expect(details.getByLabel('Question for the agent', { exact: true })).toBeFocused();
  await details.getByRole('button', { name: 'Approve proposal', exact: true }).click();
  await expect(details).toContainText('keeps correcting issues until it merges');
  // A poll must not replace the version already shown in an open decision confirmation.
  dispatch.database('data/platform/accounts.sqlite', (db) => {
    const previous = db
      .prepare('SELECT body FROM support_proposals WHERE ticket_id=? AND version=1')
      .get(ticketId)!;
    const next = { ...JSON.parse(String(previous.body)), version: 2, createdAt: Date.now() };
    db.prepare(
      'INSERT INTO support_proposals(ticket_id,version,body,created_at) VALUES(?,?,?,?)',
    ).run(ticketId, 2, JSON.stringify(next), Date.now());
    db.prepare('UPDATE support_tickets SET proposal_version=2 WHERE id=?').run(ticketId);
  });
  await expect(details).toContainText('This is an earlier proposal.', { timeout: 10000 });
  await expect(
    details.getByRole('button', { name: 'Approve and implement', exact: true }),
  ).toHaveCount(0);
  await details.getByLabel('Proposal version').selectOption('2');
  await details.getByRole('button', { name: 'Approve proposal', exact: true }).click();
  await expect(details).toContainText('Approve version 2');
  await details.getByRole('button', { name: 'Approve and implement', exact: true }).click();
  await expect(details).toContainText('Version 2 approved for implementation through merge.');
  await expect(details).toContainText('Implementing');
  await page.reload();
  await expect(details).toContainText('Version 2 approved for implementation through merge.');
  expect(new URL(page.url()).hash).toContain(ticketId);
  expect(errors).toEqual([]);
});

test('intake outcomes show agent responses and unclear reports focus the shared reply', async ({
  page,
  dispatch,
}) => {
  const member = await dispatch.client(demo.member);
  const cases = [
    {
      status: 'acknowledged',
      kind: 'bug',
      title: 'Just testing support',
      response: 'Thanks for trying AI Support. Your message was received.',
    },
    {
      status: 'answered',
      kind: 'feature',
      title: 'Where are account settings?',
      response: 'Choose Settings in the sidebar.',
    },
    {
      status: 'unclear_bug',
      kind: 'bug',
      title: 'Something looks wrong',
      response: 'I could not identify the affected control. What did you click?',
    },
    {
      status: 'unclear_feature',
      kind: 'feature',
      title: 'Make it better',
      response: 'Which task should the feature help you complete?',
    },
  ];
  // The backend holds routing and agent authority. Arrange completed outcomes here to guard
  // their actual page presentation and the clarification control's focus/channel behavior.
  for (const item of cases) {
    const created = await member.post('/api/support/tickets', {
      requestId: `intake-browser-${item.status}`,
      kind: item.kind,
      title: item.title,
      description: item.title,
      attachmentIds: [],
    });
    expect(created.status).toBe(201);
    const id = created.value.ticket.id;
    dispatch.database('data/platform/accounts.sqlite', (db) => {
      db.prepare('UPDATE support_tickets SET status=?,progress=? WHERE id=?').run(
        item.status,
        'Agent response available',
        id,
      );
      db.prepare(
        "UPDATE support_jobs SET status='done',checkpoint=? WHERE ticket_id=? AND kind='investigate'",
      ).run(
        JSON.stringify({
          phase: 'intake_completed',
          intake: {
            outcome: item.status,
            reply: item.response,
            reason: `Owner-only intake context for ${item.title}`,
            findings: item.status.startsWith('unclear_')
              ? ['No actionable change could be established from the message.']
              : [],
            questions: item.status.startsWith('unclear_') ? [item.response] : [],
            investigationAttempted: item.status.startsWith('unclear_'),
          },
        }),
        id,
      );
      db.prepare(
        "INSERT INTO support_messages(id,ticket_id,author,role,body,private,created_at) VALUES(?,?,'AI Support','agent',?,0,?)",
      ).run(`message-${item.status}`, id, item.response, Date.now());
      db.prepare(
        'INSERT INTO support_notifications(id,ticket_id,user_id,user_scope,body,created_at) SELECT ?,id,reporter_id,reporter_scope,?,? FROM support_tickets WHERE id=?',
      ).run(`notice-${item.status}`, item.title, Date.now(), id);
    });
  }
  await login(page);
  await page.getByRole('link', { name: 'Support', exact: true }).click();
  const details = page.getByRole('region', { name: 'Ticket details' });
  for (const item of cases) {
    await page.getByRole('combobox', { name: 'Filter ticket status' }).selectOption(item.status);
    await page.getByRole('button', { name: new RegExp(item.title) }).click();
    await expect(details.getByRole('list', { name: 'Ticket conversation' })).toContainText(
      item.response,
    );
    await expect(details.getByRole('region', { name: 'Private intake review' })).toContainText(
      `Owner-only intake context for ${item.title}`,
    );
    await expect(details.getByRole('heading', { name: 'Investigation in progress' })).toHaveCount(
      0,
    );
    await expect(
      details.getByRole('button', { name: 'Approve proposal', exact: true }),
    ).toHaveCount(0);
    if (item.status.startsWith('unclear_')) {
      await expect(details).toContainText(
        item.kind === 'bug' ? 'Unclear Bug Report' : 'Unclear Feature',
      );
      await details.getByRole('combobox', { name: 'Send to', exact: true }).selectOption('agent');
      await details.getByRole('button', { name: 'Add details', exact: true }).click();
      await expect(details.getByRole('combobox', { name: 'Send to', exact: true })).toHaveValue(
        'reporter',
      );
      await expect(details.getByLabel('Reply', { exact: true })).toBeFocused();
    }
  }
  await login(page, demo.member);
  await page.getByLabel('Account menu', { exact: true }).click();
  await page.getByRole('button', { name: 'AI Support', exact: true }).click();
  const panel = page.getByRole('dialog', { name: 'AI Support', exact: true });
  await panel.getByRole('tab', { name: 'Updates' }).click();
  await panel.getByRole('button', { name: /Something looks wrong/ }).click();
  await expect(panel).toContainText('Unclear Bug Report');
  await expect(panel.getByRole('list', { name: 'Ticket conversation' })).toContainText(
    cases[2]!.response,
  );
  await panel.getByRole('button', { name: 'Add details', exact: true }).click();
  await expect(panel.getByLabel('Reply', { exact: true })).toBeFocused();
});

test('a DSP whose AI Support the platform owner hides loses the button until it is shown', async ({
  page,
  browser,
}) => {
  await login(page);
  await page
    .getByRole('region', { name: 'DSPs', exact: true })
    .getByRole('button', { name: /Northline Logistics/ })
    .click();
  const pane = page.getByRole('region', { name: 'Northline Logistics', exact: true });
  await pane.getByRole('tab', { name: 'Features', exact: true }).click();
  const shown = pane.getByRole('switch', { name: 'AI Support', exact: true });
  const context = await browser.newContext();
  const member = await context.newPage();
  await login(member, demo.member);
  const menu = member.getByLabel('Account menu', { exact: true });
  const button = member.getByRole('button', { name: 'AI Support', exact: true });
  await menu.click();
  await expect(button).toBeVisible();
  await member.keyboard.press('Escape');

  await shown.click();
  await expect(shown).not.toBeChecked();
  // The member's next request finds their view expired, and the button gone.
  await member.reload();
  await menu.click();
  await expect(member.getByRole('button', { name: 'Sign out', exact: true })).toBeVisible();
  await expect(button).toHaveCount(0);
  await member.keyboard.press('Escape');
  // The platform owner keeps it.
  await page.getByLabel('Account menu', { exact: true }).click();
  await expect(page.getByRole('button', { name: 'AI Support', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');

  await shown.click();
  await expect(shown).toBeChecked();
  await member.reload();
  await menu.click();
  await expect(button).toBeVisible();
  await context.close();
});
