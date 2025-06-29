module MyMod
  VALUE = 42

  class Foo
    def bar
      VALUE
    end
  end
end

include MyMod

foo = MyMod::Foo.new
puts MyMod::VALUE
