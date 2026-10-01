// Activation-owned handles shared across the extension modules: the language
// client, the output channel, and the workspace editor state. `activate`
// assigns them; the index tree, stub extraction, and `deactivate` read them.
const session = {
    outputChannel: undefined,
    client: undefined,
    editorState: undefined
};

module.exports = { session };
