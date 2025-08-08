module GoshPosh
  module Platform
    module SpecHelpers

      DEFAULT_POST_PARAMS = {
        :price => 10,
        :category => :'Dresses & Skirts',
        :brand => :'H&M',
        :size => :'S',
        :description => :'A dress',
        :catalog => {
          :department => '000e8975d97b4e80ef00a955',
          :category => '00108975d97b4e80ef00a955',
          :category_features => [],
        }.freeze,
      }.freeze

      COLLEGE_COVERSHOT_IMAGE = '/goshposh/server/tasks/data/channels/img-covershot-channel-colleges@2x.jpg'.freeze

      def services
        Platform::PlatformServices.service
      end

      def other_services
        Platform::PlatformServices.another_service
      end

    end

    class PlatformServices
      def self.service
        "platform service"
      end

      def self.another_service
        "another service"
      end
    end
  end
end

# Usage examples
helper = GoshPosh::Platform::SpecHelpers.new
helper.services

# Direct call
GoshPosh::Platform::PlatformServices.service

# Another usage
service_instance = GoshPosh::Platform::PlatformServices.service