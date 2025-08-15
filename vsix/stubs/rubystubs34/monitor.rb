# frozen_string_literal: true

class Monitor
  # Enters exclusive section.
  def enter; end

  # Leaves exclusive section.
  def exit; end

  # Enters exclusive section and executes the block.  Leaves the exclusive
  # section automatically when the block exits.  See example under
  # +MonitorMixin+.
  def synchronize; end

  # Attempts to enter exclusive section.  Returns +false+ if lock fails.
  def try_enter; end
end
