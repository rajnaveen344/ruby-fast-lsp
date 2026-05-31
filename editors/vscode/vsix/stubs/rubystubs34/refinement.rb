# frozen_string_literal: true

class Refinement < Module
  # Imports methods from modules. Unlike Module#include,
  # Refinement#import_methods copies methods and adds them into the refinement,
  # so the refinement is activated in the imported methods.
  #
  # Note that due to method copying, only methods defined in Ruby code can be imported.
  #
  #    module StrUtils
  #      def indent(level)
  #        ' ' * level + self
  #      end
  #    end
  #
  #    module M
  #      refine String do
  #        import_methods StrUtils
  #      end
  #    end
  #
  #    using M
  #    "foo".indent(3)
  #    #=> "   foo"
  #
  #    module M
  #      refine String do
  #        import_methods Enumerable
  #        # Can't import method which is not defined with Ruby code: Enumerable#drop
  #      end
  #    end
  def import_methods(*args) end

  # Return the class or module refined by the receiver.
  #
  #    module M
  #      refine String do
  #      end
  #    end
  #
  #    M.refinements[0].target # => String
  def target; end
end
