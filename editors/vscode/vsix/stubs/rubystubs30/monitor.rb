# frozen_string_literal: true

class Monitor
  def enter; end

  def exit; end

  def mon_check_owner; end

  def mon_locked?; end

  def mon_owned?; end

  def synchronize; end

  def try_enter; end

  def wait_for_cond(p1, p2) end
end
