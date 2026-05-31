# frozen_string_literal: true

# ---- initialization ----
module TclTkLib
  # ---------------------------------------------------------------
  COMPILE_INFO = _
  FINALIZE_PROC_NAME = _
  RELEASE_DATE = _
  WINDOWING_SYSTEM = _

  def self._conv_listelement(p1) end

  def self._fromUTF8(p1, p2 = v2) end

  def self._merge_tklist(*args) end

  def self._split_tklist(p1) end

  def self._subst_Tcl_backslash(p1) end

  def self._subst_UTF_backslash(p1) end

  def self._toUTF8(p1, p2 = v2) end

  def self.do_one_event(*args) end

  def self.do_thread_callback(p1 = v1) end

  def self.encoding; end

  def self.encoding=(p1) end

  def self.encoding_system; end

  def self.encoding_system=(p1) end

  def self.get_eventloop_tick; end

  def self.get_eventloop_weight; end

  def self.get_eventloop_window_mode; end

  def self.get_no_event_wait; end

  def self.get_release_type_name(*args) end

  def self.get_version(*args) end

  # execute Tk_MainLoop
  def self.mainloop(p1 = v1) end

  def self.mainloop_abort_on_exception; end

  def self.mainloop_abort_on_exception=(p1) end

  def self.mainloop_thread?; end

  def self.mainloop_watchdog(p1 = v1) end

  def self.num_of_mainwindows; end

  def self.set_eventloop_tick(p1) end

  def self.set_eventloop_weight(p1, p2) end

  def self.set_eventloop_window_mode(p1) end

  def self.set_max_block_time(p1) end

  def self.set_no_event_wait(p1) end

  private

  def _conv_listelement(p1) end

  def _fromUTF8(p1, p2 = v2) end

  def _merge_tklist(*args) end

  def _split_tklist(p1) end

  def _subst_Tcl_backslash(p1) end

  def _subst_UTF_backslash(p1) end

  def _toUTF8(p1, p2 = v2) end

  def do_one_event(*args) end

  def do_thread_callback(p1 = v1) end

  def encoding_system; end
  alias encoding encoding_system

  def encoding_system=(p1) end
  alias encoding= encoding_system=

  def get_eventloop_tick; end

  def get_eventloop_weight; end

  def get_eventloop_window_mode; end

  def get_no_event_wait; end

  def get_release_type_name(*args) end

  def get_version(*args) end

  # execute Tk_MainLoop
  def mainloop(p1 = v1) end

  def mainloop_abort_on_exception; end

  def mainloop_abort_on_exception=(p1) end

  def mainloop_thread?; end

  def mainloop_watchdog(p1 = v1) end

  def num_of_mainwindows; end

  def set_eventloop_tick(p1) end

  def set_eventloop_weight(p1, p2) end

  def set_eventloop_window_mode(p1) end

  def set_max_block_time(p1) end

  def set_no_event_wait(p1) end

  module EventFlag
    ALL = _
    DONT_WAIT = _
    FILE = _
    IDLE = _
    # ---------------------------------------------------------------
    NONE = _
    TIMER = _
    WINDOW = _
  end

  module RELEASE_TYPE
    ALPHA = _
    BETA = _
    FINAL = _
  end

  module VarAccessFlag
    APPEND_VALUE = _
    GLOBAL_ONLY = _
    LEAVE_ERR_MSG = _
    LIST_ELEMENT = _
    NAMESPACE_ONLY = _
    # ---------------------------------------------------------------
    NONE = _
    PARSE_VARNAME = _
  end
end
