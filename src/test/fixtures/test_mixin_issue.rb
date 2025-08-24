# Test case to reproduce the mixin goto definition issue with inheritance

module GoshPosh
  module Platform
    module API
      def api_method
        puts "API method called"
      end
      
      def another_api_method
        puts "Another API method"
      end
    end

    module CookieHelpers
      def set_cookie(name, value)
        puts "Setting cookie: #{name} = #{value}"
      end
      
      def get_cookie(name)
        puts "Getting cookie: #{name}"
      end
    end
  end

  class Base
    # Include modules with helper methods
    include Platform::API
    include Platform::CookieHelpers

    def base_method
      # Try to call included methods from base class
      api_method        # Line 32: This should goto Platform::API#api_method
      set_cookie("test", "value")  # Line 33: This should goto Platform::CookieHelpers#set_cookie
    end
    
    def another_base_method
      another_api_method  # Line 37: This should goto Platform::API#another_api_method
      get_cookie("test")  # Line 38: This should goto Platform::CookieHelpers#get_cookie
    end
  end
end

class PlatformApp < GoshPosh::Base
  def app_method
    # Try to call inherited included methods from child class
    api_method        # Line 45: This should goto Platform::API#api_method
    set_cookie("app", "data")    # Line 46: This should goto Platform::CookieHelpers#set_cookie
    base_method       # Line 47: This should goto GoshPosh::Base#base_method
  end
  
  def another_app_method
    # More inherited included methods
    another_api_method  # Line 51: This should goto Platform::API#another_api_method
    get_cookie("app")   # Line 52: This should goto Platform::CookieHelpers#get_cookie
    another_base_method # Line 53: This should goto GoshPosh::Base#another_base_method
  end
end

# Test instantiation
app = PlatformApp.new
app.app_method
app.another_app_method