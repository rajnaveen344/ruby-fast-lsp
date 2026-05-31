# frozen_string_literal: true

class CGI
  module Escape
    # Returns URL-escaped string (+application/x-www-form-urlencoded+).
    def escape(string) end

    # Returns HTML-escaped string.
    def escapeHTML(string) end

    # Returns URL-escaped string following RFC 3986.
    def escapeURIComponent(string) end
    alias escape_uri_component escapeURIComponent

    # Returns URL-unescaped string (+application/x-www-form-urlencoded+).
    def unescape(string, encoding = @@accept_charset) end

    # Returns HTML-unescaped string.
    def unescapeHTML(string) end

    # Returns URL-unescaped string following RFC 3986.
    def unescapeURIComponent(string, encoding = @@accept_charset) end
    alias unescape_uri_component unescapeURIComponent
  end

  module Util
  end
end
