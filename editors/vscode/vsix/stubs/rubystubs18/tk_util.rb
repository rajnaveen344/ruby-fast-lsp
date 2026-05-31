# frozen_string_literal: true

module TkUtil
  None = _
  # ---------------------
  RELEASE_DATE = _

  def self._conv_args(*args) end

  def self._get_eval_enc_str(p1) end

  def self._get_eval_string(p1, p2 = v2) end

  def self._symbolkey2str(p1) end

  # /
  def self.bool(p1) end

  def self.callback(p1, *args) end

  # /
  def self.eval_cmd(p1, *args) end

  def self.hash_kv(*args) end

  def self.install_cmd(p1 = v1) end

  def self.num_or_str(p1) end

  def self.number(p1) end

  def self.string(p1) end

  def self.uninstall_cmd(p1) end

  def _conv_args(*args) end

  def _fromUTF8(*args) end

  def _get_eval_enc_str(p1) end

  def _get_eval_string(p1, p2 = v2) end

  def _symbolkey2str(p1) end

  def _toUTF8(*args) end

  # /
  def bool(p1) end

  def hash_kv(*args) end

  def num_or_str(p1) end

  def number(p1) end

  def string(p1) end

  # ---------------------
  class CallbackSubst
    def self._define_attribute_aliases(p1) end

    def self._get_all_subst_keys; end

    def self._get_extra_args_tbl; end

    def self._get_subst_key(p1) end

    def self._setup_subst_table(p1, p2, p3 = v3) end

    def self._sym2subst(p1) end

    def self.inspect; end

    def self.ret_val(p1) end

    def self.scan_args(p1, p2) end

    def self.subst_arg(*args) end

    def initialize(*args) end

    class Info
      def self.inspect; end
    end
  end
end
