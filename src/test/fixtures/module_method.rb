module Loggable
  def log(message)
    log_level
    puts "[LOG] #{message}"
  end

  private

  def log_level
    puts "DEBUG"
  end
end

class Logger
  include Loggable
  
  def initialize
    log("Logger initialized")
  end
end
