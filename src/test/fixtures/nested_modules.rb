module Foo
  module Bar
    module Baz
      ABC = 123

      def self.foo
        "foo"
      end
    end
  end

  puts Foo::Bar::Baz
end

puts Foo::Bar::Baz.foo
puts Foo::Bar::Baz::ABC

Foo::Bar::Baz.foo
Foo::Bar::Baz::ABC
Foo::Bar::Baz::module_method
