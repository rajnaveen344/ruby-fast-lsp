# frozen_string_literal: true

# The Warning module contains a single method named #warn, and the
# module extends itself, making <code>Warning.warn</code> available.
# Warning.warn is called for all warnings issued by Ruby.
# By default, warnings are printed to $stderr.
#
# By overriding Warning.warn, you can change how warnings are
# handled by Ruby, either filtering some warnings, and/or outputting
# warnings somewhere other than $stderr.  When Warning.warn is
# overridden, super can be called to get the default behavior of
# printing the warning to $stderr.
module Warning
  # Writes warning message +msg+ to $stderr, followed by a newline
  # if the message does not end in a newline.  This method is called
  # by Ruby for all emitted warnings.
  def warn(msg) end
end
