# frozen_string_literal: true

class Ripper
  # newline significant, +/- is an operator.
  EXPR_ARG = _
  # equals to <tt>(EXPR_ARG | EXPR_CMDARG)</tt>
  EXPR_ARG_ANY = _
  # ignore newline, +/- is a sign.
  EXPR_BEG = _
  # equals to <tt>(EXPR_BEG | EXPR_MID | EXPR_CLASS)</tt>
  EXPR_BEG_ANY = _
  # immediate after `class', no here document.
  EXPR_CLASS = _
  # newline significant, +/- is an operator.
  EXPR_CMDARG = _
  # right after `.' or `::', no reserved words.
  EXPR_DOT = _
  # newline significant, +/- is an operator.
  EXPR_END = _
  # ditto, and unbound braces.
  EXPR_ENDARG = _
  # ditto, and unbound braces.
  EXPR_ENDFN = _
  # equals to <tt>(EXPR_END | EXPR_ENDARG | EXPR_ENDFN)</tt>
  EXPR_END_ANY = _
  # symbol literal as FNAME.
  EXPR_FITEM = _
  # ignore newline, no reserved words.
  EXPR_FNAME = _
  # flag bit, label is allowed.
  EXPR_LABEL = _
  # flag bit, just after a label.
  EXPR_LABELED = _
  # newline significant, +/- is an operator.
  EXPR_MID = _
  # equals to +0+
  EXPR_NONE = _
  # equals to +EXPR_BEG+
  EXPR_VALUE = _
  # version of Ripper
  Version = _

  # USE OF RIPPER LIBRARY ONLY.
  #
  # Strips up to +width+ leading whitespaces from +input+,
  # and returns the stripped column width.
  def self.dedent_string(input, width) end

  # Returns a string representation of lex_state.
  def self.lex_state_name(integer) end

  # Create a new Ripper object.
  # _src_ must be a String, an IO, or an Object which has #gets method.
  #
  # This method does not starts parsing.
  # See also Ripper#parse and Ripper.parse.
  def initialize(p1, p2 = v2, p3 = v3) end

  # Return column number of current parsing line.
  # This number starts from 0.
  def column; end

  # Return encoding of the source.
  def encoding; end

  # Return true if parsed source ended by +\_\_END\_\_+.
  def end_seen?; end

  # Return true if parsed source has errors.
  def error?; end

  # Return current parsing filename.
  def filename; end

  # Return line number of current parsing line.
  # This number starts from 1.
  def lineno; end

  # Start parsing and returns the value of the root action.
  def parse; end

  # Return scanner state of current token.
  def state; end

  # Get yydebug.
  def yydebug; end

  # Set yydebug.
  def yydebug=(flag) end

  private

  # USE OF RIPPER LIBRARY ONLY.
  #
  # Strips up to +width+ leading whitespaces from +input+,
  # and returns the stripped column width.
  def dedent_string(input, width) end
end
