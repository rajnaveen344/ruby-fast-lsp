# frozen_string_literal: true

# Raised when encountering Ruby code with an invalid syntax.
#
#    eval("1+1=2")
#
# <em>raises the exception:</em>
#
#    SyntaxError: (eval):1: syntax error, unexpected '=', expecting $end
class SyntaxError < ScriptError
end
