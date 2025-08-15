# frozen_string_literal: true

class LocalJumpError < StandardError
  # call_seq:
  #   local_jump_error.exit_value  => obj
  #
  # Returns the exit value associated with this +LocalJumpError+.
  def exit_value; end

  # The reason this block was terminated:
  # :break, :redo, :retry, :next, :return, or :noreason.
  def reason; end
end
