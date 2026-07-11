const { getLanguageService } = require('vscode-html-languageservice');
const { TextDocument } = require('vscode-languageserver-textdocument');

const htmlLanguageService = getLanguageService();

function erbRanges(source) {
    const ranges = [];
    let cursor = 0;
    while (cursor < source.length) {
        const start = source.indexOf('<%', cursor);
        if (start < 0) break;
        const close = source.indexOf('%>', start + 2);
        const end = close < 0 ? source.length : close + 2;
        ranges.push({ start, end });
        cursor = end;
    }
    return ranges;
}

function projectErbToHtml(source) {
    const projected = source.split('');
    for (const range of erbRanges(source)) {
        for (let offset = range.start; offset < range.end; offset += 1) {
            const code = source.charCodeAt(offset);
            if (code !== 10 && code !== 13) projected[offset] = ' ';
        }
    }
    const html = projected.join('');
    if (html.length !== source.length) {
        throw new Error(
            'INVARIANT VIOLATED: ERB HTML projection changed UTF-16 length. ' +
            'Fix: mask every non-newline code unit one-for-one.'
        );
    }
    return html;
}

function createErbHtmlDocument(uri, version, source) {
    const ranges = erbRanges(source);
    const textDocument = TextDocument.create(uri, 'html', version, projectErbToHtml(source));
    const htmlDocument = htmlLanguageService.parseHTMLDocument(textDocument);
    const inRubyRange = (position) => {
        const offset = textDocument.offsetAt(position);
        return ranges.some((range) => range.start <= offset && offset < range.end);
    };

    return {
        complete(position) {
            if (inRubyRange(position)) return { isIncomplete: false, items: [] };
            return htmlLanguageService.doComplete(textDocument, position, htmlDocument);
        },
        hover(position) {
            if (inRubyRange(position)) return null;
            return htmlLanguageService.doHover(textDocument, position, htmlDocument);
        },
        symbols() {
            return htmlLanguageService.findDocumentSymbols(textDocument, htmlDocument);
        },
        foldingRanges() {
            return htmlLanguageService.getFoldingRanges(textDocument);
        },
        selectionRanges(positions) {
            return positions.map((position) => {
                if (inRubyRange(position)) {
                    return { range: { start: position, end: position } };
                }
                return htmlLanguageService.getSelectionRanges(textDocument, [position])[0];
            });
        },
        highlights(position) {
            if (inRubyRange(position)) return [];
            return htmlLanguageService.findDocumentHighlights(textDocument, position, htmlDocument);
        }
    };
}

function markdown(vscode, contents) {
    if (typeof contents === 'string') return new vscode.MarkdownString(contents);
    if (contents && typeof contents.value === 'string') {
        return new vscode.MarkdownString(contents.value);
    }
    if (Array.isArray(contents)) {
        return contents.map((content) => markdown(vscode, content));
    }
    return new vscode.MarkdownString('');
}

function range(vscode, value) {
    return new vscode.Range(
        value.start.line,
        value.start.character,
        value.end.line,
        value.end.character
    );
}

function completion(vscode, item) {
    const result = new vscode.CompletionItem(
        item.label,
        item.kind === undefined ? undefined : item.kind - 1
    );
    result.detail = item.detail;
    result.documentation = item.documentation && markdown(vscode, item.documentation);
    result.filterText = item.filterText;
    result.sortText = item.sortText;
    result.preselect = item.preselect;
    const insertText = item.textEdit ? item.textEdit.newText : item.insertText;
    if (insertText !== undefined) {
        result.insertText = item.insertTextFormat === 2
            ? new vscode.SnippetString(insertText)
            : insertText;
    }
    if (item.textEdit) result.range = range(vscode, item.textEdit.range);
    return result;
}

function documentSymbol(vscode, symbol) {
    if (symbol.location) {
        return new vscode.SymbolInformation(
            symbol.name,
            symbol.kind - 1,
            symbol.containerName || '',
            new vscode.Location(
                vscode.Uri.parse(symbol.location.uri),
                range(vscode, symbol.location.range)
            )
        );
    }
    const result = new vscode.DocumentSymbol(
        symbol.name,
        symbol.detail || '',
        symbol.kind - 1,
        range(vscode, symbol.range),
        range(vscode, symbol.selectionRange)
    );
    result.children = (symbol.children || []).map((child) => documentSymbol(vscode, child));
    return result;
}

function selectionRange(vscode, value) {
    return new vscode.SelectionRange(
        range(vscode, value.range),
        value.parent ? selectionRange(vscode, value.parent) : undefined
    );
}

function foldingRangeKind(vscode, kind) {
    if (kind === 'comment') return vscode.FoldingRangeKind.Comment;
    if (kind === 'imports') return vscode.FoldingRangeKind.Imports;
    if (kind === 'region') return vscode.FoldingRangeKind.Region;
    return undefined;
}

function registerErbHtmlProviders(vscode, context, reportError = () => {}) {
    const selector = { language: 'erb', scheme: 'file' };
    const feature = (document) => createErbHtmlDocument(
        document.uri.toString(),
        document.version,
        document.getText()
    );
    const safe = async (name, fallback, callback) => {
        try {
            return await callback();
        } catch (error) {
            reportError(`ERB HTML ${name} failed: ${error.message}`);
            return fallback;
        }
    };

    const providers = [
        vscode.languages.registerCompletionItemProvider(selector, {
            provideCompletionItems(document, position) {
                return safe('completion', [], () =>
                    feature(document).complete(position).items.map((item) => completion(vscode, item))
                );
            }
        }, '<', '/', ' ', '"', "'", '=', ':', '-'),
        vscode.languages.registerHoverProvider(selector, {
            provideHover(document, position) {
                return safe('hover', undefined, () => {
                    const value = feature(document).hover(position);
                    return value && new vscode.Hover(
                        markdown(vscode, value.contents),
                        value.range && range(vscode, value.range)
                    );
                });
            }
        }),
        vscode.languages.registerDocumentSymbolProvider(selector, {
            provideDocumentSymbols(document) {
                return safe('document symbols', [], () =>
                    feature(document).symbols().map((symbol) => documentSymbol(vscode, symbol))
                );
            }
        }),
        vscode.languages.registerFoldingRangeProvider(selector, {
            provideFoldingRanges(document) {
                return safe('folding ranges', [], () =>
                    feature(document).foldingRanges().map((value) =>
                        new vscode.FoldingRange(
                            value.startLine,
                            value.endLine,
                            foldingRangeKind(vscode, value.kind)
                        )
                    )
                );
            }
        }),
        vscode.languages.registerSelectionRangeProvider(selector, {
            provideSelectionRanges(document, positions) {
                return safe('selection ranges', [], () =>
                    feature(document).selectionRanges(positions).map((value) =>
                        selectionRange(vscode, value)
                    )
                );
            }
        }),
        vscode.languages.registerDocumentHighlightProvider(selector, {
            provideDocumentHighlights(document, position) {
                return safe('document highlights', [], () =>
                    feature(document).highlights(position).map((value) =>
                        new vscode.DocumentHighlight(
                            range(vscode, value.range),
                            value.kind === undefined ? undefined : value.kind - 1
                        )
                    )
                );
            }
        })
    ];
    context.subscriptions.push(...providers);
}

module.exports = {
    createErbHtmlDocument,
    projectErbToHtml,
    registerErbHtmlProviders
};
