module Utilities
  def self.helper_method
    puts "Helper method called"
  end

  class Config
    attr_reader :settings

    def initialize
      @settings = {}
    end

    def add_setting(key, value)
      @settings[key] = value
    end
  end
end

class App
  include Utilities

  VERSION = "1.0.0"

  def initialize
    @config = Utilities::Config.new
  end

  def start
    puts "Starting app version #{VERSION}"
  end

  def configure
    yield @config if block_given?
  end
end
