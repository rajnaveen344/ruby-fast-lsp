# frozen_string_literal: true

# Raised when a file required (a Ruby script, extension library, ...)
# fails to load.
#
#    require 'this/file/does/not/exist'
#
# <em>raises the exception:</em>
#
#    LoadError: no such file to load -- this/file/does/not/exist
class LoadError < ScriptError
end
