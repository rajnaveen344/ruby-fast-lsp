module GoshPosh
  module Platform
    class PlatformApp < GoshPosh::Base
      helpers do
        def local_helper
        end
      end

      get "/x" do
        get_consignment_request_inventory_images(1, 2, 3)
      end
    end
  end
end
