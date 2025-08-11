# frozen_string_literal: true

# Raised when attempting a potential unsafe operation, typically when
# the $SAFE level is raised above 0.
#
#    foo = "bar"
#    proc = Proc.new do
#      $SAFE = 4
#      foo.gsub! "a", "*"
#    end
#    proc.call
#
# <em>raises the exception:</em>
#
#    SecurityError: Insecure: can't modify string
class SecurityError < Exception
end
