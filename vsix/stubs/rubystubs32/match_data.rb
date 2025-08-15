# frozen_string_literal: true

# MatchData encapsulates the result of matching a Regexp against
# string. It is returned by Regexp#match and String#match, and also
# stored in a global variable returned by Regexp.last_match.
#
# Usage:
#
#     url = 'https://docs.ruby-lang.org/en/2.5.0/MatchData.html'
#     m = url.match(/(\d\.?)+/)   # => #<MatchData "2.5.0" 1:"0">
#     m.string                    # => "https://docs.ruby-lang.org/en/2.5.0/MatchData.html"
#     m.regexp                    # => /(\d\.?)+/
#     # entire matched substring:
#     m[0]                        # => "2.5.0"
#
#     # Working with unnamed captures
#     m = url.match(%r{([^/]+)/([^/]+)\.html$})
#     m.captures                  # => ["2.5.0", "MatchData"]
#     m[1]                        # => "2.5.0"
#     m.values_at(1, 2)           # => ["2.5.0", "MatchData"]
#
#     # Working with named captures
#     m = url.match(%r{(?<version>[^/]+)/(?<module>[^/]+)\.html$})
#     m.captures                  # => ["2.5.0", "MatchData"]
#     m.named_captures            # => {"version"=>"2.5.0", "module"=>"MatchData"}
#     m[:version]                 # => "2.5.0"
#     m.values_at(:version, :module)
#                                 # => ["2.5.0", "MatchData"]
#     # Numerical indexes are working, too
#     m[1]                        # => "2.5.0"
#     m.values_at(1, 2)           # => ["2.5.0", "MatchData"]
#
# == Global variables equivalence
#
# Parts of last MatchData (returned by Regexp.last_match) are also
# aliased as global variables:
#
# * <code>$~</code> is Regexp.last_match;
# * <code>$&</code> is Regexp.last_match<code>[ 0 ]</code>;
# * <code>$1</code>, <code>$2</code>, and so on are
#   Regexp.last_match<code>[ i ]</code> (captures by number);
# * <code>$`</code> is Regexp.last_match<code>.pre_match</code>;
# * <code>$'</code> is Regexp.last_match<code>.post_match</code>;
# * <code>$+</code> is Regexp.last_match<code>[ -1 ]</code> (the last capture).
#
# See also "Special global variables" section in Regexp documentation.
class MatchData
  # When arguments +index+, +start and +length+, or +range+ are given,
  # returns match and captures in the style of Array#[]:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m[0] # => "HX1138"
  #   m[1, 2]  # => ["H", "X"]
  #   m[1..3]  # => ["H", "X", "113"]
  #   m[-3, 2] # => ["X", "113"]
  #
  # When string or symbol argument +name+ is given,
  # returns the matched substring for the given name:
  #
  #   m = /(?<foo>.)(.)(?<bar>.+)/.match("hoge")
  #   # => #<MatchData "hoge" foo:"h" bar:"ge">
  #   m['foo'] # => "h"
  #   m[:bar]  # => "ge"
  def [](...) end

  # Returns the offset (in characters) of the beginning of the specified match.
  #
  # When non-negative integer argument +n+ is given,
  # returns the offset of the beginning of the <tt>n</tt>th match:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m[0]       # => "HX1138"
  #   m.begin(0) # => 1
  #   m[3]       # => "113"
  #   m.begin(3) # => 3
  #
  #   m = /(т)(е)(с)/.match('тест')
  #   # => #<MatchData "тес" 1:"т" 2:"е" 3:"с">
  #   m[0]       # => "тес"
  #   m.begin(0) # => 0
  #   m[3]       # => "с"
  #   m.begin(3) # => 2
  #
  # When string or symbol argument +name+ is given,
  # returns the offset of the beginning for the named match:
  #
  #   m = /(?<foo>.)(.)(?<bar>.)/.match("hoge")
  #   # => #<MatchData "hog" foo:"h" bar:"g">
  #   m[:foo]        # => "h"
  #   m.begin('foo') # => 0
  #   m[:bar]        # => "g"
  #   m.begin(:bar)  # => 2
  #
  # Related: MatchData#end, MatchData#offset, MatchData#byteoffset.
  def begin(...) end

  # Returns a two-element array containing the beginning and ending byte-based offsets of
  # the <em>n</em>th match.
  # <em>n</em> can be a string or symbol to reference a named capture.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.byteoffset(0)      #=> [1, 7]
  #    m.byteoffset(4)      #=> [6, 7]
  #
  #    m = /(?<foo>.)(.)(?<bar>.)/.match("hoge")
  #    p m.byteoffset(:foo) #=> [0, 1]
  #    p m.byteoffset(:bar) #=> [2, 3]
  def byteoffset(n) end

  # Returns the array of captures,
  # which are all matches except <tt>m[0]</tt>:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m[0]       # => "HX1138"
  #   m.captures # => ["H", "X", "113", "8"]
  #
  # Related: MatchData.to_a.
  def captures; end
  alias deconstruct captures

  # Returns a hash of the named captures for the given names.
  #
  #   m = /(?<hours>\d{2}):(?<minutes>\d{2}):(?<seconds>\d{2})/.match("18:37:22")
  #   m.deconstruct_keys([:hours, :minutes]) # => {:hours => "18", :minutes => "37"}
  #   m.deconstruct_keys(nil) # => {:hours => "18", :minutes => "37", :seconds => "22"}
  #
  # Returns an empty hash of no named captures were defined:
  #
  #   m = /(\d{2}):(\d{2}):(\d{2})/.match("18:37:22")
  #   m.deconstruct_keys(nil) # => {}
  def deconstruct_keys(array_of_names) end

  # Returns the offset (in characters) of the end of the specified match.
  #
  # When non-negative integer argument +n+ is given,
  # returns the offset of the end of the <tt>n</tt>th match:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m[0]     # => "HX1138"
  #   m.end(0) # => 7
  #   m[3]     # => "113"
  #   m.end(3) # => 6
  #
  #   m = /(т)(е)(с)/.match('тест')
  #   # => #<MatchData "тес" 1:"т" 2:"е" 3:"с">
  #   m[0]     # => "тес"
  #   m.end(0) # => 3
  #   m[3]     # => "с"
  #   m.end(3) # => 3
  #
  # When string or symbol argument +name+ is given,
  # returns the offset of the end for the named match:
  #
  #   m = /(?<foo>.)(.)(?<bar>.)/.match("hoge")
  #   # => #<MatchData "hog" foo:"h" bar:"g">
  #   m[:foo]      # => "h"
  #   m.end('foo') # => 1
  #   m[:bar]      # => "g"
  #   m.end(:bar)  # => 3
  #
  # Related: MatchData#begin, MatchData#offset, MatchData#byteoffset.
  def end(...) end

  # Returns +true+ if +object+ is another \MatchData object
  # whose target string, regexp, match, and captures
  # are the same as +self+, +false+ otherwise.
  #
  # MatchData#eql? is an alias for MatchData#==.
  def eql?(other) end
  alias == eql?

  # Returns the integer hash value for +self+,
  # based on the target string, regexp, match, and captures.
  #
  # See also Object#hash.
  def hash; end

  # Returns a string representation of +self+:
  #
  #    m = /.$/.match("foo")
  #    # => #<MatchData "o">
  #    m.inspect # => "#<MatchData \"o\">"
  #
  #    m = /(.)(.)(.)/.match("foo")
  #    # => #<MatchData "foo" 1:"f" 2:"o" 3:"o">
  #    m.inspect # => "#<MatchData \"foo\" 1:\"f\" 2:\"o\
  #
  #    m = /(.)(.)?(.)/.match("fo")
  #    # => #<MatchData "fo" 1:"f" 2:nil 3:"o">
  #    m.inspect # => "#<MatchData \"fo\" 1:\"f\" 2:nil 3:\"o\">"
  #
  #  Related: MatchData#to_s.
  def inspect; end

  # Returns the matched substring corresponding to the given argument.
  #
  # When non-negative argument +n+ is given,
  # returns the matched substring for the <tt>n</tt>th match:
  #
  #   m = /(.)(.)(\d+)(\d)(\w)?/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8" 5:nil>
  #   m.match(0) # => "HX1138"
  #   m.match(4) # => "8"
  #   m.match(5) # => nil
  #
  # When string or symbol argument +name+ is given,
  # returns the matched substring for the given name:
  #
  #   m = /(?<foo>.)(.)(?<bar>.+)/.match("hoge")
  #   # => #<MatchData "hoge" foo:"h" bar:"ge">
  #   m.match('foo') # => "h"
  #   m.match(:bar)  # => "ge"
  def match(...) end

  # Returns the length (in characters) of the matched substring
  # corresponding to the given argument.
  #
  # When non-negative argument +n+ is given,
  # returns the length of the matched substring
  # for the <tt>n</tt>th match:
  #
  #   m = /(.)(.)(\d+)(\d)(\w)?/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8" 5:nil>
  #   m.match_length(0) # => 6
  #   m.match_length(4) # => 1
  #   m.match_length(5) # => nil
  #
  # When string or symbol argument +name+ is given,
  # returns the length of the matched substring
  # for the named match:
  #
  #   m = /(?<foo>.)(.)(?<bar>.+)/.match("hoge")
  #   # => #<MatchData "hoge" foo:"h" bar:"ge">
  #   m.match_length('foo') # => 1
  #   m.match_length(:bar)  # => 2
  def match_length(...) end

  # Returns a hash of the named captures;
  # each key is a capture name; each value is its captured string or +nil+:
  #
  #   m = /(?<foo>.)(.)(?<bar>.+)/.match("hoge")
  #   # => #<MatchData "hoge" foo:"h" bar:"ge">
  #   m.named_captures # => {"foo"=>"h", "bar"=>"ge"}
  #
  #   m = /(?<a>.)(?<b>.)/.match("01")
  #   # => #<MatchData "01" a:"0" b:"1">
  #   m.named_captures #=> {"a" => "0", "b" => "1"}
  #
  #   m = /(?<a>.)(?<b>.)?/.match("0")
  #   # => #<MatchData "0" a:"0" b:nil>
  #   m.named_captures #=> {"a" => "0", "b" => nil}
  #
  #   m = /(?<a>.)(?<a>.)/.match("01")
  #   # => #<MatchData "01" a:"0" a:"1">
  #   m.named_captures #=> {"a" => "1"}
  def named_captures; end

  # Returns an array of the capture names
  # (see {Named Captures}[rdoc-ref:Regexp@Named+Captures]):
  #
  #   m = /(?<foo>.)(?<bar>.)(?<baz>.)/.match("hoge")
  #   # => #<MatchData "hog" foo:"h" bar:"o" baz:"g">
  #   m.names # => ["foo", "bar", "baz"]
  #
  #   m = /foo/.match('foo') # => #<MatchData "foo">
  #   m.names # => [] # No named captures.
  #
  # Equivalent to:
  #
  #   m = /(?<foo>.)(?<bar>.)(?<baz>.)/.match("hoge")
  #   m.regexp.names # => ["foo", "bar", "baz"]
  def names; end

  # Returns a 2-element array containing the beginning and ending
  # offsets (in characters) of the specified match.
  #
  # When non-negative integer argument +n+ is given,
  # returns the starting and ending offsets of the <tt>n</tt>th match:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m[0]        # => "HX1138"
  #   m.offset(0) # => [1, 7]
  #   m[3]        # => "113"
  #   m.offset(3) # => [3, 6]
  #
  #   m = /(т)(е)(с)/.match('тест')
  #   # => #<MatchData "тес" 1:"т" 2:"е" 3:"с">
  #   m[0]        # => "тес"
  #   m.offset(0) # => [0, 3]
  #   m[3]        # => "с"
  #   m.offset(3) # => [2, 3]
  #
  # When string or symbol argument +name+ is given,
  # returns the starting and ending offsets for the named match:
  #
  #   m = /(?<foo>.)(.)(?<bar>.)/.match("hoge")
  #   # => #<MatchData "hog" foo:"h" bar:"g">
  #   m[:foo]         # => "h"
  #   m.offset('foo') # => [0, 1]
  #   m[:bar]         # => "g"
  #   m.offset(:bar)  # => [2, 3]
  #
  # Related: MatchData#byteoffset, MatchData#begin, MatchData#end.
  def offset(...) end

  # Returns the substring of the target string from
  # the end of the first match in +self+ (that is, <tt>self[0]</tt>)
  # to the end of the string;
  # equivalent to regexp global variable <tt>$'</tt>:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138: The Movie")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m[0]         # => "HX1138"
  #   m.post_match # => ": The Movie"\
  #
  # Related: MatchData.pre_match.
  def post_match; end

  # Returns the substring of the target string from its beginning
  # up to the first match in +self+ (that is, <tt>self[0]</tt>);
  # equivalent to regexp global variable <tt>$`</tt>:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m[0]        # => "HX1138"
  #   m.pre_match # => "T"
  #
  # Related: MatchData#post_match.
  def pre_match; end

  # Returns the regexp that produced the match:
  #
  #   m = /a.*b/.match("abc") # => #<MatchData "ab">
  #   m.regexp                # => /a.*b/
  def regexp; end

  # Returns size of the match array:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m.size # => 5
  #
  # MatchData#length is an alias for MatchData.size.
  def size; end
  alias length size

  # Returns the target string if it was frozen;
  # otherwise, returns a frozen copy of the target string:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m.string # => "THX1138."
  def string; end

  # Returns the array of matches:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m.to_a # => ["HX1138", "H", "X", "113", "8"]
  #
  # Related: MatchData#captures.
  def to_a; end

  # Returns the matched string:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m.to_s # => "HX1138"
  #
  #   m = /(?<foo>.)(.)(?<bar>.+)/.match("hoge")
  #   # => #<MatchData "hoge" foo:"h" bar:"ge">
  #   m.to_s # => "hoge"
  #
  # Related: MatchData.inspect.
  def to_s; end

  # Returns match and captures at the given +indexes+,
  # which may include any mixture of:
  #
  # - Integers.
  # - Ranges.
  # - Names (strings and symbols).
  #
  # Examples:
  #
  #   m = /(.)(.)(\d+)(\d)/.match("THX1138: The Movie")
  #   # => #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #   m.values_at(0, 2, -2) # => ["HX1138", "X", "113"]
  #   m.values_at(1..2, -1) # => ["H", "X", "8"]
  #
  #   m = /(?<a>\d+) *(?<op>[+\-*\/]) *(?<b>\d+)/.match("1 + 2")
  #   # => #<MatchData "1 + 2" a:"1" op:"+" b:"2">
  #   m.values_at(0, 1..2, :a, :b, :op)
  #   # => ["1 + 2", "1", "+", "1", "2", "+"]
  def values_at(*indexes) end
end
