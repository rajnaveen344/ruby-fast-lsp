module Alpha::Beta::Gamma
    ABC = 99
  
    class Foo
      def bar
        ABC
      end
    end
end

Alpha::Beta::Gamma::ABC
Alpha::Beta::Gamma::Foo.new