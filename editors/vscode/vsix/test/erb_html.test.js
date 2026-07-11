const assert = require('node:assert/strict');
const test = require('node:test');

const {
    createErbHtmlDocument,
    projectErbToHtml,
    registerErbHtmlProviders
} = require('../erb_html');

test('ERB HTML projection preserves UTF-16 offsets and host markup', () => {
    const source = '<main>😀</main>\n<%= user.名前 %>\n<footer id="page">Done</footer>\n';
    const projected = projectErbToHtml(source);

    assert.equal(projected.length, source.length);
    assert.equal(projected.slice(0, source.indexOf('<%')), '<main>😀</main>\n');
    assert.equal(projected.slice(source.indexOf('<%'), source.indexOf('%>') + 2).trim(), '');
    assert.equal(projected.slice(source.indexOf('%>') + 2), '\n<footer id="page">Done</footer>\n');
});

test('ERB HTML projection masks comments, escaped tags, and unclosed tags', () => {
    const source = '<div><%# comment %><%% escaped %><%= value %></div>\n<% unclosed';
    const projected = projectErbToHtml(source);

    assert.equal(projected.length, source.length);
    assert.equal(projected.replace(/ /g, ''), '<div></div>\n');
});

test('HTML language features work in host markup without leaking into Ruby', () => {
    const source = '<main>\n  <section cl></section>\n  <%= User.name %>\n</main>\n';
    const document = createErbHtmlDocument('file:///app/views/users/show.html.erb', 1, source);

    const hostItems = document.complete({ line: 1, character: 13 }).items;
    assert(hostItems.some((item) => item.label === 'class'));

    const rubyItems = document.complete({ line: 2, character: 10 }).items;
    assert.equal(rubyItems.length, 0);

    const symbols = document.symbols();
    assert(symbols.some((symbol) => symbol.name === 'main'));
    assert(symbols.some((symbol) => symbol.name === 'section'));

    assert(document.hover({ line: 0, character: 2 }));
    assert.equal(document.hover({ line: 2, character: 8 }), null);
    assert(document.foldingRanges().some((range) => range.startLine === 0));
    assert.equal(document.highlights({ line: 1, character: 4 }).length, 2);
    assert.equal(document.highlights({ line: 2, character: 8 }).length, 0);

    const rubySelection = document.selectionRanges([{ line: 2, character: 8 }])[0];
    assert.deepEqual(rubySelection.range, {
        start: { line: 2, character: 8 },
        end: { line: 2, character: 8 }
    });
    assert.equal(rubySelection.parent, undefined);
});

test('VS Code adapter registers host providers and converts flat HTML symbols', async () => {
    const registered = {};
    const disposable = { dispose() {} };
    class Range {
        constructor(startLine, startCharacter, endLine, endCharacter) {
            this.start = { line: startLine, character: startCharacter };
            this.end = { line: endLine, character: endCharacter };
        }
    }
    class Location {
        constructor(uri, range) {
            this.uri = uri;
            this.range = range;
        }
    }
    class SymbolInformation {
        constructor(name, kind, containerName, location) {
            Object.assign(this, { name, kind, containerName, location });
        }
    }
    class FoldingRange {
        constructor(start, end, kind) {
            Object.assign(this, { start, end, kind });
        }
    }
    const vscode = {
        languages: {
            registerCompletionItemProvider(_selector, provider) {
                registered.completion = provider;
                return disposable;
            },
            registerHoverProvider(_selector, provider) {
                registered.hover = provider;
                return disposable;
            },
            registerDocumentSymbolProvider(_selector, provider) {
                registered.symbols = provider;
                return disposable;
            },
            registerFoldingRangeProvider(_selector, provider) {
                registered.folding = provider;
                return disposable;
            },
            registerSelectionRangeProvider(_selector, provider) {
                registered.selection = provider;
                return disposable;
            },
            registerDocumentHighlightProvider(_selector, provider) {
                registered.highlights = provider;
                return disposable;
            }
        },
        Range,
        Location,
        SymbolInformation,
        FoldingRange,
        FoldingRangeKind: { Comment: 1, Imports: 2, Region: 3 },
        Uri: { parse: (value) => value }
    };
    const context = { subscriptions: [] };
    registerErbHtmlProviders(vscode, context);

    assert.equal(context.subscriptions.length, 6);
    const symbols = await registered.symbols.provideDocumentSymbols({
        uri: { toString: () => 'file:///app/views/show.html.erb' },
        version: 1,
        getText: () => '<main><section></section></main>'
    });
    assert(symbols.every((symbol) => symbol instanceof SymbolInformation));
    assert.deepEqual(symbols.map((symbol) => symbol.name), ['main', 'section']);
    assert.equal(symbols[1].containerName, 'main');

    const folding = await registered.folding.provideFoldingRanges({
        uri: { toString: () => 'file:///app/views/show.html.erb' },
        version: 1,
        getText: () => '<!--\ncomment\n-->'
    });
    assert.deepEqual(folding, [new FoldingRange(0, 2, 1)]);
});
