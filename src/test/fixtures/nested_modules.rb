module Foo
  module Bar
    module Baz
      ABC = 123

      def self.foo
        "foo"
      end
    end
  end

  Foo::Bar::Baz
end

puts Foo::Bar::Baz.foo
puts Foo::Bar::Baz::ABC
