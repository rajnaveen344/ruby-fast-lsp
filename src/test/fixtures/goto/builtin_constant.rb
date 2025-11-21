# Define a top-level constant
class TopLevelClass
end

module A
end

module B
  include A
  # Reference to TopLevelClass from within module B
  a = TopLevelClass
end

class C_A
  include B
  # Reference to TopLevelClass from within class C_A
  a = TopLevelClass

  # Reference to TopLevelClass in a conditional
  if a.class == TopLevelClass
  end
end
