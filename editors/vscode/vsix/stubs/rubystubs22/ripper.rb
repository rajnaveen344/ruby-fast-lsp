# frozen_string_literal: true

class Ripper
  # version of Ripper
  Version = _

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

  # Get yydebug.
  def yydebug; end

  # Set yydebug.
  def yydebug=(flag) end
end
