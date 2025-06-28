module Loggable
  def log(message)
    puts "[LOG] #{message}"
  end
end

module Taggable
  def tag(name)
    @tags ||= []
    @tags << name
  end
end

class MyService
  include Loggable
  extend Taggable

  def perform_action
    log("Action performed")
  end
end

MyService.tag("important")

module Prependable
  def greet
    "Hello from Prependable!"
  end
end

class Greeter
  prepend Prependable

  def greet
    "Hello from Greeter!"
  end
end

class TestGreeter
  def test
    g = Greeter.new
    g.greet
  end
end
