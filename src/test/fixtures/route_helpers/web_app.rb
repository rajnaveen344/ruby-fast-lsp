module ExampleApp
  module Services
    class WebApp < ExampleApp::BaseApp
      helpers do
        def local_helper
        end
      end

      get "/entries" do
        list_entries(1, 2, 3)
      end
    end
  end
end
