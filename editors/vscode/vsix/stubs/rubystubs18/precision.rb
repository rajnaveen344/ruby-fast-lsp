# frozen_string_literal: true

# Precision is a mixin for concrete numeric classes with
# precision.  Here, `precision' means the fineness of approximation
# of a real number, so, this module should not be included into
# anything which is not a subset of Real (so it should not be
# included in classes such as +Complex+ or +Matrix+).
module Precision
  # call_seq:
  #   included
  #
  # When the +Precision+ module is mixed-in to a class, this +included+
  # method is used to add our default +induced_from+ implementation
  # to the host class.
  def self.included(p1) end

  # Converts _self_ into an instance of _klass_. By default,
  # +prec+ invokes
  #
  #    klass.induced_from(num)
  #
  # and returns its value. So, if <code>klass.induced_from</code>
  # doesn't return an instance of _klass_, it will be necessary
  # to reimplement +prec+.
  def prec(klass) end

  # Returns a +Float+ converted from _num_. It is equivalent
  # to <code>prec(Float)</code>.
  def prec_f; end

  # Returns an +Integer+ converted from _num_. It is equivalent
  # to <code>prec(Integer)</code>.
  def prec_i; end
end
