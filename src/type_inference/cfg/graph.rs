//! Control Flow Graph data structures.
//!
//! This module defines the core CFG types used for type narrowing analysis.

use std::collections::{HashMap, HashSet};

use crate::type_inference::ruby_type::RubyType;

use super::guards::TypeGuard;

/// Unique identifier for a basic block
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

impl BlockId {
    pub fn new(id: usize) -> Self {
        Self(id)
    }
}

/// A basic block - a sequence of statements with single entry/exit
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Unique identifier
    pub id: BlockId,
    /// Statements in this block
    pub statements: Vec<Statement>,
    /// Type guards that apply when entering this block
    pub entry_guards: Vec<TypeGuard>,
    /// Source location range for this block
    pub location: BlockLocation,
}

impl BasicBlock {
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            statements: Vec::new(),
            entry_guards: Vec::new(),
            location: BlockLocation::default(),
        }
    }

    pub fn with_guard(mut self, guard: TypeGuard) -> Self {
        self.entry_guards.push(guard);
        self
    }

    pub fn add_statement(&mut self, stmt: Statement) {
        self.statements.push(stmt);
    }
}

/// A statement within a basic block
#[derive(Debug, Clone)]
pub struct Statement {
    /// Byte range in source
    pub start_offset: usize,
    pub end_offset: usize,
    /// Kind of statement
    pub kind: StatementKind,
}

impl Statement {
    pub fn new(start_offset: usize, end_offset: usize, kind: StatementKind) -> Self {
        Self {
            start_offset,
            end_offset,
            kind,
        }
    }

    pub fn assignment(
        start: usize,
        end: usize,
        target: String,
        value_type: Option<RubyType>,
    ) -> Self {
        Self::new(
            start,
            end,
            StatementKind::Assignment {
                target,
                value_type,
                source_variable: None,
            },
        )
    }

    pub fn assignment_from_variable(
        start: usize,
        end: usize,
        target: String,
        source_variable: String,
    ) -> Self {
        Self::new(
            start,
            end,
            StatementKind::Assignment {
                target,
                value_type: None,
                source_variable: Some(source_variable),
            },
        )
    }

    pub fn expression(start: usize, end: usize) -> Self {
        Self::new(start, end, StatementKind::Expression)
    }

    pub fn return_stmt(start: usize, end: usize, value_type: Option<RubyType>) -> Self {
        Self::new(start, end, StatementKind::Return { value_type })
    }
}

/// Kind of statement
#[derive(Debug, Clone)]
pub enum StatementKind {
    /// Variable assignment: x = value
    Assignment {
        target: String,
        /// Direct type if known (e.g., from literal)
        value_type: Option<RubyType>,
        /// Source variable if assigned from another variable (e.g., b = a)
        source_variable: Option<String>,
    },
    /// Or assignment: x = a || b (union of both types)
    OrAssignment {
        target: String,
        /// Left operand variable name (if it's a variable)
        left_var: Option<String>,
        /// Left operand type (if it's a literal)
        left_type: Option<RubyType>,
        /// Right operand variable name (if it's a variable)
        right_var: Option<String>,
        /// Right operand type (if it's a literal)
        right_type: Option<RubyType>,
    },
    /// And assignment: x = a && b (right type or falsy)
    AndAssignment {
        target: String,
        /// Left operand variable name (if it's a variable)
        left_var: Option<String>,
        /// Left operand type (if it's a literal)
        left_type: Option<RubyType>,
        /// Right operand variable name (if it's a variable)
        right_var: Option<String>,
        /// Right operand type (if it's a literal)
        right_type: Option<RubyType>,
    },
    /// Method call (for tracking side effects)
    MethodCall {
        receiver: Option<String>,
        method: String,
    },
    /// Return statement
    Return { value_type: Option<RubyType> },
    /// Generic expression
    Expression,
}

/// Location information for a block
#[derive(Debug, Clone, Default)]
pub struct BlockLocation {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub start_offset: usize,
    pub end_offset: usize,
}

impl BlockLocation {
    pub fn new(
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        start_offset: usize,
        end_offset: usize,
    ) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
            start_offset,
            end_offset,
        }
    }

    /// Create a BlockLocation from offsets only (line/col set to 0)
    /// Use this when line/col are not needed for performance
    pub fn from_offsets(start_offset: usize, end_offset: usize) -> Self {
        Self {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            start_offset,
            end_offset,
        }
    }

    /// Check if a position is within this location
    pub fn contains(&self, line: u32, col: u32) -> bool {
        if line < self.start_line || line > self.end_line {
            return false;
        }
        if line == self.start_line && col < self.start_col {
            return false;
        }
        if line == self.end_line && col > self.end_col {
            return false;
        }
        true
    }
}

/// Edge between basic blocks
#[derive(Debug, Clone)]
pub struct CfgEdge {
    pub from: BlockId,
    pub to: BlockId,
    pub kind: EdgeKind,
}

impl CfgEdge {
    pub fn unconditional(from: BlockId, to: BlockId) -> Self {
        Self {
            from,
            to,
            kind: EdgeKind::Unconditional,
        }
    }

    pub fn conditional_true(from: BlockId, to: BlockId, guard: TypeGuard) -> Self {
        Self {
            from,
            to,
            kind: EdgeKind::ConditionalTrue(guard),
        }
    }

    pub fn conditional_false(from: BlockId, to: BlockId, guard: TypeGuard) -> Self {
        Self {
            from,
            to,
            kind: EdgeKind::ConditionalFalse(guard),
        }
    }

    pub fn exception(from: BlockId, to: BlockId) -> Self {
        Self {
            from,
            to,
            kind: EdgeKind::Exception,
        }
    }
}

/// Kind of edge between blocks
#[derive(Debug, Clone)]
pub enum EdgeKind {
    /// Unconditional jump (fallthrough or goto)
    Unconditional,
    /// Conditional true branch
    ConditionalTrue(TypeGuard),
    /// Conditional false branch
    ConditionalFalse(TypeGuard),
    /// Exception handler edge
    Exception,
    /// Return from method
    Return,
}

/// The Control Flow Graph
#[derive(Debug)]
pub struct ControlFlowGraph {
    /// All basic blocks
    pub blocks: HashMap<BlockId, BasicBlock>,
    /// Entry block
    pub entry: BlockId,
    /// Exit blocks (may have multiple due to returns)
    pub exits: Vec<BlockId>,
    /// Forward edges (block → successors)
    pub successors: HashMap<BlockId, Vec<CfgEdge>>,
    /// Backward edges (block → predecessors)
    pub predecessors: HashMap<BlockId, Vec<BlockId>>,
    /// Method parameters with their initial types
    pub parameters: Vec<(String, RubyType)>,
    /// Next block ID to allocate
    next_block_id: usize,
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            entry: BlockId(0),
            exits: Vec::new(),
            successors: HashMap::new(),
            predecessors: HashMap::new(),
            parameters: Vec::new(),
            next_block_id: 0,
        }
    }

    /// Create a new block and return its ID
    pub fn create_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block_id);
        self.next_block_id += 1;
        self.blocks.insert(id, BasicBlock::new(id));
        id
    }

    /// Create a new block with a type guard
    pub fn create_block_with_guard(&mut self, guard: TypeGuard) -> BlockId {
        let id = self.create_block();
        if let Some(block) = self.blocks.get_mut(&id) {
            block.entry_guards.push(guard);
        }
        id
    }

    /// Get a mutable reference to a block
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(&id)
    }

    /// Get a reference to a block
    pub fn get_block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(&id)
    }

    /// Add an edge between blocks
    pub fn add_edge(&mut self, edge: CfgEdge) {
        self.successors
            .entry(edge.from)
            .or_default()
            .push(edge.clone());
        self.predecessors
            .entry(edge.to)
            .or_default()
            .push(edge.from);
    }

    /// Mark a block as an exit block
    pub fn mark_exit(&mut self, block: BlockId) {
        if !self.exits.contains(&block) {
            self.exits.push(block);
        }
    }

    /// Check if CFG has back edges (loops)
    pub fn has_back_edges(&self) -> bool {
        // A CFG has loops if any successor points to an already-visited node during DFS
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();
        self.detect_back_edge(self.entry, &mut visited, &mut in_stack)
    }

    fn detect_back_edge(
        &self,
        block: BlockId,
        visited: &mut HashSet<BlockId>,
        in_stack: &mut HashSet<BlockId>,
    ) -> bool {
        if in_stack.contains(&block) {
            return true; // Back edge found
        }
        if visited.contains(&block) {
            return false;
        }
        visited.insert(block);
        in_stack.insert(block);

        if let Some(successors) = self.successors.get(&block) {
            for edge in successors {
                if self.detect_back_edge(edge.to, visited, in_stack) {
                    return true;
                }
            }
        }

        in_stack.remove(&block);
        false
    }

    /// Get all blocks in reverse post-order (for dataflow analysis)
    pub fn reverse_post_order(&self) -> Vec<BlockId> {
        let mut visited = HashSet::new();
        let mut post_order = Vec::new();
        self.dfs_post_order(self.entry, &mut visited, &mut post_order);
        post_order.reverse();
        post_order
    }

    fn dfs_post_order(
        &self,
        block: BlockId,
        visited: &mut HashSet<BlockId>,
        post_order: &mut Vec<BlockId>,
    ) {
        if visited.contains(&block) {
            return;
        }
        visited.insert(block);

        if let Some(successors) = self.successors.get(&block) {
            for edge in successors {
                self.dfs_post_order(edge.to, visited, post_order);
            }
        }

        post_order.push(block);
    }

    /// Get successors of a block
    pub fn get_successors(&self, block: BlockId) -> &[CfgEdge] {
        self.successors
            .get(&block)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get predecessors of a block
    pub fn get_predecessors(&self, block: BlockId) -> &[BlockId] {
        self.predecessors
            .get(&block)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Find the block containing a given source position
    pub fn find_block_at_position(&self, line: u32, col: u32) -> Option<BlockId> {
        for (id, block) in &self.blocks {
            if block.location.contains(line, col) {
                return Some(*id);
            }
        }
        None
    }

    /// Get the number of blocks
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get the number of edges
    pub fn edge_count(&self) -> usize {
        self.successors.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_cfg() {
        let mut cfg = ControlFlowGraph::new();

        let entry = cfg.create_block();
        let then_block = cfg.create_block();
        let else_block = cfg.create_block();
        let merge = cfg.create_block();

        cfg.entry = entry;

        // entry -> then (true branch)
        cfg.add_edge(CfgEdge::conditional_true(
            entry,
            then_block,
            TypeGuard::IsNil {
                variable: "x".to_string(),
            },
        ));

        // entry -> else (false branch)
        cfg.add_edge(CfgEdge::conditional_false(
            entry,
            else_block,
            TypeGuard::IsNil {
                variable: "x".to_string(),
            },
        ));

        // then -> merge
        cfg.add_edge(CfgEdge::unconditional(then_block, merge));

        // else -> merge
        cfg.add_edge(CfgEdge::unconditional(else_block, merge));

        cfg.mark_exit(merge);

        assert_eq!(cfg.block_count(), 4);
        assert_eq!(cfg.edge_count(), 4);
        assert_eq!(cfg.exits.len(), 1);
    }

    #[test]
    fn test_reverse_post_order() {
        let mut cfg = ControlFlowGraph::new();

        let b0 = cfg.create_block();
        let b1 = cfg.create_block();
        let b2 = cfg.create_block();
        let b3 = cfg.create_block();

        cfg.entry = b0;

        // Linear flow: b0 -> b1 -> b2 -> b3
        cfg.add_edge(CfgEdge::unconditional(b0, b1));
        cfg.add_edge(CfgEdge::unconditional(b1, b2));
        cfg.add_edge(CfgEdge::unconditional(b2, b3));

        let rpo = cfg.reverse_post_order();
        assert_eq!(rpo, vec![b0, b1, b2, b3]);
    }

    #[test]
    fn test_block_location_contains() {
        let loc = BlockLocation::new(10, 5, 15, 20, 100, 200);

        // Inside
        assert!(loc.contains(12, 10));

        // On start line, after start col
        assert!(loc.contains(10, 10));

        // On end line, before end col
        assert!(loc.contains(15, 15));

        // Before start
        assert!(!loc.contains(9, 0));

        // After end
        assert!(!loc.contains(16, 0));

        // On start line, before start col
        assert!(!loc.contains(10, 2));
    }
}
