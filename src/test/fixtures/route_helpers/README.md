# Route helper regression

This synthetic fixture tests navigation through a helper block, an inherited
class, a reopened nested API module, and a sibling namespace with the same short
name. Unresolved optional mixins keep the incomplete-lookup case represented.
The integration test repeats indexing of the current web-app buffer and checks
that the helper definition remains reachable.
