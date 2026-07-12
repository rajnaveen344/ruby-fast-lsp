const policy = require('./ruby_file_kinds.json');
const RUBY_EXTENSIONS = policy.rubyExtensions;
const RUBY_FILENAMES = policy.rubyFilenames;
const ERB_EXTENSIONS = policy.erbExtensions;

function fileWatcherPatterns(includedPatterns = []) {
    const extensions = [...RUBY_EXTENSIONS, ...ERB_EXTENSIONS]
        .map((extension) => extension.slice(1));
    return [
        `**/*.{${extensions.join(',')}}`,
        `**/{${RUBY_FILENAMES.join(',')}}`,
        ...includedPatterns.filter((pattern) => typeof pattern === 'string' && pattern.length > 0)
    ];
}

module.exports = {
    ERB_EXTENSIONS,
    RUBY_EXTENSIONS,
    RUBY_FILENAMES,
    fileWatcherPatterns
};
