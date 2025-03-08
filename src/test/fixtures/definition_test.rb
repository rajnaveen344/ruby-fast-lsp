class DefinitionTest
  def self.class_method
    puts "Class method"
  end

  def instance_method
    puts "Instance method"
  end
end

test = DefinitionTest.new
test.instance_method
DefinitionTest.class_method
