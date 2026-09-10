'use strict';

// Loaded only by the real VS Code extension host through --extensionTestsPath.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const CHECKS = ['activate-exact-vsix', 'open-ruby-documents', 'ruby-definition', 'require-navigation',
  'ruby-hover', 'ruby-references', 'unsaved-multi-file-edit', 'restore-buffers', 'erb-ruby-definition', 'erb-html-completion'];

function locations(result) {
  return (Array.isArray(result) ? result : result ? [result] : []).map(location => ({
    uri: location.targetUri || location.uri,
    range: location.targetSelectionRange || location.range || location.targetRange,
  }));
}

function targets(result, document, line) {
  return locations(result).some(location => location.uri?.toString() === document.uri.toString() && location.range?.start.line === line);
}

function tokenPosition(document, token) {
  const offset = document.getText().lastIndexOf(token);
  assert.ok(offset >= 0, `Fixture document must contain token ${token}; fix the test coordinates.`);
  return document.positionAt(offset + Math.floor(token.length / 2));
}

async function observe(label, query, accepted, timeout = 15000) {
  const deadline = Date.now() + timeout;
  let last;
  do {
    let timer;
    try {
      last = await Promise.race([query(), new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`${label}: provider did not answer within the observation deadline.`)), Math.max(1, deadline - Date.now()));
      })]);
    } finally { clearTimeout(timer); }
    if (accepted(last)) return last;
    if (Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, Math.min(200, deadline - Date.now())));
  } while (Date.now() < deadline);
  throw new Error(`${label}: no acceptable provider result within ${timeout}ms. Last result: ${JSON.stringify(last)}. Fix the provider or indexing lifecycle; empty results are not success.`);
}

async function run() {
  const resultFile = process.env.RUBY_FAST_LSP_VSCODE_RESULT;
  const fixture = process.env.RUBY_FAST_LSP_VSCODE_FIXTURE;
  assert.ok(resultFile && fixture, 'Run this test through smoke_vscode.js so isolated fixtures and evidence paths are supplied.');
  const report = { schema_version: 1, run_id: process.env.RUBY_FAST_LSP_VSCODE_RUN_ID,
    started_at: new Date().toISOString(), vscode_version: vscode.version, status: 'running', checks: [],
    unexercised: ['server restart', 'runtime selection UI', 'formatting and linting', 'test command execution', 'workspace folder add/remove'] };
  const persist = () => fs.writeFileSync(resultFile, JSON.stringify(report, null, 2) + '\n');
  const check = async (name, action) => {
    const started = Date.now();
    try {
      const details = await action();
      report.checks.push({ name, status: 'passed', elapsed_ms: Date.now() - started, details });
    } catch (error) {
      report.checks.push({ name, status: 'failed', elapsed_ms: Date.now() - started, error: error.stack || error.message });
      throw error;
    } finally { persist(); }
  };
  let service;
  let main;
  let originalService;
  let originalMain;
  const definition = (document, token) => vscode.commands.executeCommand('vscode.executeDefinitionProvider', document.uri, tokenPosition(document, token));
  const restore = async () => {
    const edit = new vscode.WorkspaceEdit();
    for (const [document, original] of [[service, originalService], [main, originalMain]]) {
      edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), original);
    }
    assert.equal(await vscode.workspace.applyEdit(edit), true, 'Restoring the original buffers must succeed.');
    for (const [document, original] of [[service, originalService], [main, originalMain]]) {
      assert.equal(document.getText(), original, 'The restored buffer must exactly match its original content.');
      assert.equal(fs.readFileSync(document.uri.fsPath, 'utf8'), original, 'Unsaved edits must never change fixture files on disk.');
    }
    await observe('restored method definition', () => definition(main, 'value'), result => targets(result, service, 1));
  };
  persist();
  try {
    await check('activate-exact-vsix', async () => {
      const extension = vscode.extensions.getExtension(process.env.RUBY_FAST_LSP_VSCODE_EXTENSION);
      assert.ok(extension, 'The exact extracted extension must be registered in the isolated host.');
      assert.equal(fs.realpathSync(extension.extensionPath), fs.realpathSync(process.env.RUBY_FAST_LSP_VSCODE_EXTENSION_ROOT), 'VS Code must activate the extracted VSIX, not another installation.');
      await extension.activate();
      assert.equal(extension.isActive, true, 'The extracted extension must activate successfully.');
      return { extension_id: extension.id, version: extension.packageJSON.version };
    });
    await check('open-ruby-documents', async () => {
      service = await vscode.workspace.openTextDocument(path.join(fixture, 'service.rb'));
      main = await vscode.workspace.openTextDocument(path.join(fixture, 'main.rb'));
      await vscode.window.showTextDocument(main, { preview: false });
      assert.equal(service.languageId, 'ruby', 'service.rb must use Ruby language services.');
      assert.equal(main.languageId, 'ruby', 'main.rb must use Ruby language services.');
      originalService = service.getText();
      originalMain = main.getText();
    });
    await check('ruby-definition', async () => {
      await observe('Ruby method definition', () => definition(main, 'value'), result => targets(result, service, 1), 30000);
      return { target: 'service.rb', line: 1 };
    });
    await check('require-navigation', async () => {
      await observe('require_relative definition', () => definition(main, 'service'), result => targets(result, service, 0));
      return { target: 'service.rb', line: 0 };
    });
    await check('ruby-hover', async () => {
      const hovers = await observe('Ruby method hover', () => vscode.commands.executeCommand('vscode.executeHoverProvider', main.uri, tokenPosition(main, 'value')),
        result => Array.isArray(result) && result.some(hover => hover.contents?.some(content => typeof content === 'string' ? content.trim() : content.value?.trim())));
      return { contents: hovers.flatMap(hover => hover.contents.map(content => typeof content === 'string' ? content : content.value)) };
    });
    await check('ruby-references', async () => {
      await observe('Ruby method references', () => vscode.commands.executeCommand('vscode.executeReferenceProvider', service.uri, tokenPosition(service, 'value')),
        result => targets(result, main, 1));
      return { includes: 'main.rb', line: 1 };
    });
    await check('unsaved-multi-file-edit', async () => {
      const edit = new vscode.WorkspaceEdit();
      for (const document of [service, main]) {
        const offset = document.getText().lastIndexOf('value');
        edit.replace(document.uri, new vscode.Range(document.positionAt(offset), document.positionAt(offset + 5)), 'changed');
      }
      assert.equal(await vscode.workspace.applyEdit(edit), true, 'The unsaved method and call edits must apply together.');
      assert.ok(service.isDirty && main.isDirty, 'Both edited Ruby documents must remain unsaved.');
      assert.equal(service.getText(), originalService.replace('value', 'changed'));
      assert.equal(main.getText(), originalMain.replace('value', 'changed'));
      assert.equal(fs.readFileSync(service.uri.fsPath, 'utf8'), originalService, 'Unsaved method edits must preserve disk content.');
      assert.equal(fs.readFileSync(main.uri.fsPath, 'utf8'), originalMain, 'Unsaved call edits must preserve disk content.');
      await observe('definition after unsaved edits', () => definition(main, 'changed'), result => targets(result, service, 1));
    });
    await check('restore-buffers', restore);
    await check('erb-ruby-definition', async () => {
      const document = await vscode.workspace.openTextDocument(path.join(fixture, 'view.html.erb'));
      await vscode.window.showTextDocument(document, { preview: false });
      assert.equal(document.languageId, 'erb', 'The installed manifest must assign ERB language services to .html.erb.');
      await observe('Ruby definition inside ERB', () => definition(document, 'value'), result => targets(result, service, 1));
      return { target: 'service.rb', line: 1 };
    });
    await check('erb-html-completion', async () => {
      const document = await vscode.workspace.openTextDocument(path.join(fixture, 'completion.html.erb'));
      await vscode.window.showTextDocument(document, { preview: false });
      assert.equal(document.languageId, 'erb', 'Host completion must run through the real ERB document selector.');
      await observe('ERB host tag completion', () => vscode.commands.executeCommand('vscode.executeCompletionItemProvider', document.uri, new vscode.Position(0, 4)),
        result => result?.items?.some(item => (typeof item.label === 'string' ? item.label : item.label.label) === 'section'));
      assert.equal(document.getText(), '<sec', 'Requesting HTML completion must not edit the template.');
      assert.equal(fs.readFileSync(path.join(fixture, 'view.html.erb'), 'utf8'), '<section><%= Service.new.value %></section>\n', 'HTML completion must preserve embedded Ruby in the separate template.');
      return { completion: 'section' };
    });
    report.status = 'passed';
  } catch (error) {
    report.status = 'failed';
    if (service && main && originalService !== undefined && originalMain !== undefined
        && (service.getText() !== originalService || main.getText() !== originalMain)) {
      try { await check('restore-buffers-after-failure', restore); }
      catch (restoreError) { throw new AggregateError([error, restoreError], 'The editor check and buffer restoration both failed.'); }
    }
    throw error;
  } finally {
    for (const name of CHECKS) {
      if (!report.checks.some(check => check.name === name)) report.checks.push({ name, status: 'not_run', reason: 'An earlier required editor check failed.' });
    }
    report.completed_at = new Date().toISOString();
    persist();
  }
}

module.exports = { run };
