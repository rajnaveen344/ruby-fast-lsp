# frozen_string_literal: true

class TclTkIp
  # initialize interpreter
  def initialize(p1 = v1, p2 = v2) end

  def _conv_listelement(p1) end

  def _create_console; end

  def _eval(p1) end

  def _fromUTF8(p1, p2 = v2) end

  def _get_global_var(p1) end

  def _get_global_var2(p1, p2) end

  def _get_variable(p1, p2) end

  def _get_variable2(p1, p2, p3) end

  def _immediate_invoke(*args) end

  def _invoke(*args) end

  def _make_menu_embeddable(p1) end

  def _merge_tklist(*args) end

  # get return code from Tcl_Eval()
  def _return_value; end

  def _set_global_var(p1, p2) end

  def _set_global_var2(p1, p2, p3) end

  def _set_variable(p1, p2, p3) end

  def _set_variable2(p1, p2, p3, p4) end

  def _split_tklist(p1) end

  def _thread_tkwait(p1, p2) end

  def _thread_vwait(p1) end

  def _toUTF8(p1, p2 = v2) end

  def _unset_global_var(p1) end

  def _unset_global_var2(p1, p2) end

  def _unset_variable(p1, p2) end

  def _unset_variable2(p1, p2, p3) end

  # allow_ruby_exit = mode
  def allow_ruby_exit=(p1) end

  # allow_ruby_exit?
  def allow_ruby_exit?; end

  def create_dummy_encoding_for_tk(p1) end

  def create_slave(p1, p2 = v2) end

  # delete interpreter
  def delete; end

  def deleted?; end

  def do_one_event(*args) end

  def encoding_table; end

  def get_eventloop_tick; end

  def get_eventloop_weight; end

  def get_no_event_wait; end

  def has_mainwindow?; end

  # is deleted?
  def invalid_namespace?; end

  def mainloop(*args) end

  def mainloop_abort_on_exception; end

  def mainloop_abort_on_exception=(p1) end

  def mainloop_watchdog(*args) end

  def make_safe; end

  def restart; end

  # is safe?
  def safe?; end

  def set_eventloop_tick(p1) end

  def set_eventloop_weight(p1, p2) end

  def set_max_block_time(p1) end

  def set_no_event_wait(p1) end

  # self is slave of master?
  def slave_of?(p1) end
end
