# frozen_string_literal: true

# The most standard error types are subclasses of StandardError. A
# rescue clause without an explicit Exception class will rescue all
# StandardErrors (and only those).
#
#    def foo
#      raise "Oups"
#    end
#    foo rescue "Hello"   #=> "Hello"
#
# On the other hand:
#
#    require 'does/not/exist' rescue "Hi"
#
# <em>raises the exception:</em>
#
#    LoadError: no such file to load -- does/not/exist
class StandardError < Exception
end
