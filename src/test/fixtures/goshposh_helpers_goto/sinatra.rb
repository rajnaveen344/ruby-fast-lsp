module Sinatra
  class Base
    def self.helpers(*args, &block)
      block.call if block
    end
    def self.get(*args, &block)
    end
    def self.use(*args)
    end
  end
end
