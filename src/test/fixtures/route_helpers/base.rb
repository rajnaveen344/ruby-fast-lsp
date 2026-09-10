module ExampleApp
  class BaseApp < Sinatra::Base
    helpers do
      include ExampleApp::Services::API
      include OptionalSessionMethods
    end
  end
end
