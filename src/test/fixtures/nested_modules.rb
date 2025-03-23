module Foo
  module Bar
    module Baz
      def self.foo
        "foo"
      end
    end
  end
end

puts Foo::Bar::Baz.foo
