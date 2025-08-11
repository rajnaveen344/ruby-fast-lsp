# frozen_string_literal: true

# <code>MatchData</code> is the type of the special variable <code>$~</code>,
# and is the type of the object returned by <code>Regexp#match</code> and
# <code>Regexp.last_match</code>. It encapsulates all the results of a pattern
# match, results normally accessed through the special variables
# <code>$&</code>, <code>$'</code>, <code>$`</code>, <code>$1</code>,
# <code>$2</code>, and so on.
class MatchData
  # Match Reference -- <code>MatchData</code> acts as an array, and may be
  # accessed using the normal array indexing techniques.  <code>mtch[0]</code>
  # is equivalent to the special variable <code>$&</code>, and returns the
  # entire matched string.  <code>mtch[1]</code>, <code>mtch[2]</code>, and so
  # on return the values of the matched backreferences (portions of the
  # pattern between parentheses).
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m          #=> #<MatchData "HX1138" 1:"H" 2:"X" 3:"113" 4:"8">
  #    m[0]       #=> "HX1138"
  #    m[1, 2]    #=> ["H", "X"]
  #    m[1..3]    #=> ["H", "X", "113"]
  #    m[-3, 2]   #=> ["X", "113"]
  #
  #    m = /(?<foo>a+)b/.match("ccaaab")
  #    m          #=> #<MatchData "aaab" foo:"aaa">
  #    m["foo"]   #=> "aaa"
  #    m[:foo]    #=> "aaa"
  def [](*several_variants) end

  # Returns the offset of the start of the <em>n</em>th element of the match
  # array in the string.
  # <em>n</em> can be a string or symbol to reference a named capture.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.begin(0)       #=> 1
  #    m.begin(2)       #=> 2
  #
  #    m = /(?<foo>.)(.)(?<bar>.)/.match("hoge")
  #    p m.begin(:foo)  #=> 0
  #    p m.begin(:bar)  #=> 2
  def begin(n) end

  # Returns the array of captures; equivalent to <code>mtch.to_a[1..-1]</code>.
  #
  #    f1,f2,f3,f4 = /(.)(.)(\d+)(\d)/.match("THX1138.").captures
  #    f1    #=> "H"
  #    f2    #=> "X"
  #    f3    #=> "113"
  #    f4    #=> "8"
  def captures; end

  # Returns the offset of the character immediately following the end of the
  # <em>n</em>th element of the match array in the string.
  # <em>n</em> can be a string or symbol to reference a named capture.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.end(0)         #=> 7
  #    m.end(2)         #=> 3
  #
  #    m = /(?<foo>.)(.)(?<bar>.)/.match("hoge")
  #    p m.end(:foo)    #=> 1
  #    p m.end(:bar)    #=> 3
  def end(n) end

  #  Equality---Two matchdata are equal if their target strings,
  #  patterns, and matched positions are identical.
  def eql?(other) end
  alias == eql?

  # Produce a hash based on the target string, regexp and matched
  # positions of this matchdata.
  #
  # See also Object#hash.
  def hash; end

  # Returns a printable version of <i>mtch</i>.
  #
  #     puts /.$/.match("foo").inspect
  #     #=> #<MatchData "o">
  #
  #     puts /(.)(.)(.)/.match("foo").inspect
  #     #=> #<MatchData "foo" 1:"f" 2:"o" 3:"o">
  #
  #     puts /(.)(.)?(.)/.match("fo").inspect
  #     #=> #<MatchData "fo" 1:"f" 2:nil 3:"o">
  #
  #     puts /(?<foo>.)(?<bar>.)(?<baz>.)/.match("hoge").inspect
  #     #=> #<MatchData "hog" foo:"h" bar:"o" baz:"g">
  def inspect; end

  # Returns a list of names of captures as an array of strings.
  # It is same as mtch.regexp.names.
  #
  #     /(?<foo>.)(?<bar>.)(?<baz>.)/.match("hoge").names
  #     #=> ["foo", "bar", "baz"]
  #
  #     m = /(?<x>.)(?<y>.)?/.match("a") #=> #<MatchData "a" x:"a" y:nil>
  #     m.names                          #=> ["x", "y"]
  def names; end

  # Returns a two-element array containing the beginning and ending offsets of
  # the <em>n</em>th match.
  # <em>n</em> can be a string or symbol to reference a named capture.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.offset(0)      #=> [1, 7]
  #    m.offset(4)      #=> [6, 7]
  #
  #    m = /(?<foo>.)(.)(?<bar>.)/.match("hoge")
  #    p m.offset(:foo) #=> [0, 1]
  #    p m.offset(:bar) #=> [2, 3]
  def offset(n) end

  # Returns the portion of the original string after the current match.
  # Equivalent to the special variable <code>$'</code>.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138: The Movie")
  #    m.post_match   #=> ": The Movie"
  def post_match; end

  # Returns the portion of the original string before the current match.
  # Equivalent to the special variable <code>$`</code>.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.pre_match   #=> "T"
  def pre_match; end

  # Returns the regexp.
  #
  #     m = /a.*b/.match("abc")
  #     m.regexp #=> /a.*b/
  def regexp; end

  # Returns the number of elements in the match array.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.length   #=> 5
  #    m.size     #=> 5
  def size; end
  alias length size

  # Returns a frozen copy of the string passed in to <code>match</code>.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.string   #=> "THX1138."
  def string; end

  # Returns the array of matches.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.to_a   #=> ["HX1138", "H", "X", "113", "8"]
  #
  # Because <code>to_a</code> is called when expanding
  # <code>*</code><em>variable</em>, there's a useful assignment
  # shortcut for extracting matched fields. This is slightly slower than
  # accessing the fields directly (as an intermediate array is
  # generated).
  #
  #    all,f1,f2,f3 = * /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    all   #=> "HX1138"
  #    f1    #=> "H"
  #    f2    #=> "X"
  #    f3    #=> "113"
  def to_a; end

  # Returns the entire matched string.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.to_s   #=> "HX1138"
  def to_s; end

  #    mtch.values_at([index]*)   -> array
  #
  # Uses each <i>index</i> to access the matching values, returning an array of
  # the corresponding matches.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138: The Movie")
  #    m.to_a               #=> ["HX1138", "H", "X", "113", "8"]
  #    m.values_at(0, 2, -2)   #=> ["HX1138", "X", "113"]
  def values_at(*args) end
end
