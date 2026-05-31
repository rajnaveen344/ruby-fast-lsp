# frozen_string_literal: true

class CGI
  module Escape
    # Returns URL-escaped string.
    def escape(string) end

    # Returns HTML-escaped string.
    def escapeHTML(string) end

    # Returns URL-unescaped string.
    def unescape(string, encoding = @@accept_charset) end

    # Returns HTML-unescaped string.
    def unescapeHTML(string) end
  end

  module Util
  end
end
