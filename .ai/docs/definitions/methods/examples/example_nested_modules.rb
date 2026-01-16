# Complex nested module example for understanding goto definition

module OuterA
  module InnerA1
    def method_a1
      puts "A1"
    end
  end

  module InnerA2
    def method_a2
      puts "A2"
    end
  end
end

module OuterB
  module InnerB1
    include OuterA::InnerA1

    def method_b1
      puts "B1"
    end

    def calls_a1
      method_a1  # <-- Goto definition here
    end
  end

  module InnerB2
    include OuterA::InnerA2

    def method_b2
      puts "B2"
    end
  end
end

class MyClass
  include OuterA::InnerA1
  include OuterB::InnerB1

  def my_method
    method_a1    # <-- Goto definition should find OuterA::InnerA1#method_a1
    method_b1    # <-- Goto definition should find OuterB::InnerB1#method_b1
    calls_a1     # <-- Goto definition should find OuterB::InnerB1#calls_a1
  end
end
