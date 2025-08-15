# frozen_string_literal: true

# <code>MatchData</code> is the type of the special variable <code>$~</code>,
# and is the type of the object returned by <code>Regexp#match</code> and
# <code>Regexp#last_match</code>. It encapsulates all the results of a pattern
# match, results normally accessed through the special variables
# <code>$&</code>, <code>$'</code>, <code>$`</code>, <code>$1</code>,
# <code>$2</code>, and so on. <code>Matchdata</code> is also known as
# <code>MatchingData</code>.
class MatchData
  # Match Reference---<code>MatchData</code> acts as an array, and may be
  # accessed using the normal array indexing techniques.  <i>mtch</i>[0] is
  # equivalent to the special variable <code>$&</code>, and returns the entire
  # matched string.  <i>mtch</i>[1], <i>mtch</i>[2], and so on return the values
  # of the matched backreferences (portions of the pattern between parentheses).
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m[0]       #=> "HX1138"
  #    m[1, 2]    #=> ["H", "X"]
  #    m[1..3]    #=> ["H", "X", "113"]
  #    m[-3, 2]   #=> ["X", "113"]
  def [](*several_variants) end

  # Returns the offset of the start of the <em>n</em>th element of the match
  # array in the string.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.begin(0)   #=> 1
  #    m.begin(2)   #=> 2
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
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.end(0)   #=> 7
  #    m.end(2)   #=> 3
  def end(n) end

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
  def inspect; end

  # Returns a two-element array containing the beginning and ending offsets of
  # the <em>n</em>th match.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138.")
  #    m.offset(0)   #=> [1, 7]
  #    m.offset(4)   #=> [6, 7]
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

  # Returns an array containing match strings for which <em>block</em>
  # gives <code>true</code>.  MatchData#select will be removed from Ruby 1.9.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138: The Movie")
  #    p m.select{|x| /X/ =~ x}   #=> ["HX1138", "X"]
  def select; end

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
  #    all,f1,f2,f3 = *(/(.)(.)(\d+)(\d)/.match("THX1138."))
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

  # Uses each <i>index</i> to access the matching values, returning an array of
  # the corresponding matches.
  #
  #    m = /(.)(.)(\d+)(\d)/.match("THX1138: The Movie")
  #    m.to_a               #=> ["HX1138", "H", "X", "113", "8"]
  #    m.values_at(0, 2, -2)   #=> ["HX1138", "X", "113"]
  def values_at(*args) end
end
