class Outer
  def outer_method
    puts "In outer method"
  end

  class Inner
    def inner_method
      puts "In inner method"
    end

    class VeryInner
      def very_inner_method
        puts "In very inner method"
      end
    end
  end
end

outer = Outer.new
outer.outer_method

inner = Outer::Inner.new
inner.inner_method

very_inner = Outer::Inner::VeryInner.new
very_inner.very_inner_method

