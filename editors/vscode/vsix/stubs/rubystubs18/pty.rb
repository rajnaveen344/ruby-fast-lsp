# frozen_string_literal: true

module PTY
  # ruby function: getpty
  def self.getpty(*args) end

  # ruby function: protect_signal - obsolete
  def self.protect_signal; end

  # ruby function: reset_signal - obsolete
  def self.reset_signal; end

  # ruby function: getpty
  def self.spawn(*args) end

  private

  # ruby function: getpty
  def getpty(*args) end
  alias spawn getpty

  # ruby function: protect_signal - obsolete
  def protect_signal; end

  # ruby function: reset_signal - obsolete
  def reset_signal; end

  class ChildExited < RuntimeError
    def status; end
  end
end
