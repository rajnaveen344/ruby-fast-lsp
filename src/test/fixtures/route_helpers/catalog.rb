module ExampleApp::Services::API
  module Catalog
    include ExampleApp::Services::Catalog::EntryHelpers
    include OptionalFormatting

    def list_entries(section, offset, limit)
      []
    end
  end
end
