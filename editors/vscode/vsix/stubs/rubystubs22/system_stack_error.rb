# frozen_string_literal: true

# Raised in case of a stack overflow.
#
#    def me_myself_and_i
#      me_myself_and_i
#    end
#    me_myself_and_i
#
# <em>raises the exception:</em>
#
#   SystemStackError: stack level too deep
class SystemStackError < Exception
end
