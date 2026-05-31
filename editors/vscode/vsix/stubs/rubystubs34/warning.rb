# frozen_string_literal: true

# The Warning module contains a single method named #warn, and the
# module extends itself, making Warning.warn available.
# Warning.warn is called for all warnings issued by Ruby.
# By default, warnings are printed to $stderr.
#
# Changing the behavior of Warning.warn is useful to customize how warnings are
# handled by Ruby, for instance by filtering some warnings, and/or outputting
# warnings somewhere other than <tt>$stderr</tt>.
#
# If you want to change the behavior of Warning.warn you should use
# <tt>Warning.extend(MyNewModuleWithWarnMethod)</tt> and you can use +super+
# to get the default behavior of printing the warning to <tt>$stderr</tt>.
#
# Example:
#   module MyWarningFilter
#     def warn(message, category: nil, **kwargs)
#       if /some warning I want to ignore/.match?(message)
#         # ignore
#       else
#         super
#       end
#     end
#   end
#   Warning.extend MyWarningFilter
#
# You should never redefine Warning#warn (the instance method), as that will
# then no longer provide a way to use the default behavior.
#
# The warning[https://rubygems.org/gems/warning] gem provides convenient ways to customize Warning.warn.
module Warning
  # Returns the flag to show the warning messages for +category+.
  # Supported categories are:
  #
  # +:deprecated+ ::
  #   deprecation warnings
  #   * assignment of non-nil value to <code>$,</code> and <code>$;</code>
  #   * keyword arguments
  #   etc.
  #
  # +:experimental+ ::
  #   experimental features
  #
  # +:performance+ ::
  #   performance hints
  #   * Shape variation limit
  def self.[](category) end

  # Sets the warning flags for +category+.
  # See Warning.[] for the categories.
  def self.[]=(category, flag) end

  # Returns a list of the supported category symbols.
  def self.categories; end

  # Writes warning message +msg+ to $stderr. This method is called by
  # Ruby for all emitted warnings. A +category+ may be included with
  # the warning.
  #
  # See the documentation of the Warning module for how to customize this.
  def warn(msg, category: nil) end
end
