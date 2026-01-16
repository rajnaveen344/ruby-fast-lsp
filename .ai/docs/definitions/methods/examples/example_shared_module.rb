# Example: Shared module included in multiple classes
# Each class implements the same method differently

module SharedModule
  def process
    log_start
    helper_method  # <-- Goto definition here should find BOTH implementations
    log_end
  end

  def log_start
    puts "Starting..."
  end

  def log_end
    puts "Done."
  end
end

class ClassA
  include SharedModule

  def helper_method
    puts "ClassA implementation"
  end

  def run
    process  # <-- This calls SharedModule#process
  end
end

class ClassB
  include SharedModule

  def helper_method
    puts "ClassB implementation"
  end

  def run
    process  # <-- This also calls SharedModule#process
  end
end

class ClassC
  include SharedModule

  def helper_method
    puts "ClassC implementation"
  end

  def run
    process
  end
end

# At runtime, which helper_method gets called depends on the receiver:
# ClassA.new.process  # Calls ClassA#helper_method
# ClassB.new.process  # Calls ClassB#helper_method
# ClassC.new.process  # Calls ClassC#helper_method

# For goto definition, the LSP should show ALL three implementations
