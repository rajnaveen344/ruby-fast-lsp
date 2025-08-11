# frozen_string_literal: true

# ::VM
class RubyVM
  # ::VM::InsnNameArray
  INSTRUCTION_NAMES = _
  OPTS = _
  # ::VM::USAGE_ANALYSIS_*
  USAGE_ANALYSIS_INSN = _
  USAGE_ANALYSIS_INSN_BIGRAM = _
  USAGE_ANALYSIS_REGS = _

  # ::VM::Env
  class Env
  end

  # declare ::RubyVM::InstructionSequence
  class InstructionSequence
    def self.compile(p1, p2 = v2, p3 = v3, p4 = v4, p5 = v5) end

    def self.compile_file(p1, p2 = v2) end

    def self.compile_option; end

    def self.compile_option=(p1) end

    def self.disasm(p1) end

    def self.disassemble(p1) end

    def self.load(p1, p2 = v2) end

    def initialize(p1, p2 = v2, p3 = v3, p4 = v4, p5 = v5) end

    def disasm; end
    alias disassemble disasm

    def eval; end

    def inspect; end

    def to_a; end
  end
end
