module Loggable
  def log(message)
    puts "[LOG] #{message}"
  end
end

module Serializable
  def serialize
    instance_variables.map do |var|
      [var, instance_variable_get(var)]
    end.to_h
  end
end

class User
  include Loggable
  include Serializable

  attr_accessor :name, :email

  def initialize(name, email)
    @name = name
    @email = email
    log("User created: #{name}")
  end

  def to_s
    "User: #{@name} (#{@email})"
  end
end

user = User.new("John", "john@example.com")
puts user.to_s
puts user.serialize.inspect
