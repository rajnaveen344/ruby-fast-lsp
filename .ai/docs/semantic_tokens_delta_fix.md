# Semantic Tokens Delta Calculation Fix

## Issue

The semantic token delta calculation in `TokenVisitor.add_token` was experiencing underflow issues when processing certain Ruby code patterns. This would result in very large delta values (close to u32::MAX) in the logs, such as:

```
Adding token: "user_service", type method, range 1568:55-1568:67, delta 0:4294967283
Adding token: "services", type method, range 1568:46-1568:54, delta 0:4294967287
```

The issue occurred when processing tokens that appeared in a non-linear order in the source code, causing the current position tracking to be ahead of a subsequent token's position. When calculating the delta (difference) between these positions, the unsigned integer subtraction would underflow.

## Root Cause Analysis

The LSP semantic tokens protocol requires tokens to be sent in a specific order (by line, then by character position), with each token's position expressed as a delta from the previous token. This makes the protocol more efficient by avoiding repetition of absolute positions.

In our implementation, we were:
1. Calculating delta_line = start_pos.line - current_position.0
2. Calculating delta_column = start_pos.character - current_position.1 (when on the same line)

However, the tree-sitter/prism parser doesn't always visit nodes in a strictly left-to-right, top-to-bottom order. This can lead to situations where we process a token that appears earlier in the source code than the last token we processed, resulting in negative deltas, which underflow when stored in unsigned integers.
