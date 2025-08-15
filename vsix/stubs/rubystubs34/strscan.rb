# frozen_string_literal: true

class StringScanner
  # Returns a new `StringScanner` object whose [stored string][1]
  # is the given `string`;
  # sets the [fixed-anchor property][10]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.string        # => "foobarbaz"
  # scanner.fixed_anchor? # => false
  # put_situation(scanner)
  # # Situation:
  # #   pos:       0
  # #   charpos:   0
  # #   rest:      "foobarbaz"
  # #   rest_size: 9
  # ```
  def initialize(string, fixed_anchor: false) end

  # Returns a captured substring or `nil`;
  # see [Captured Match Values][13].
  #
  # When there are captures:
  #
  # ```rb
  # scanner = StringScanner.new('Fri Dec 12 1975 14:39')
  # scanner.scan(/(?<wday>\w+) (?<month>\w+) (?<day>\d+) /)
  # ```
  #
  # - `specifier` zero: returns the entire matched substring:
  #
  #     ```rb
  #     scanner[0]         # => "Fri Dec 12 "
  #     scanner.pre_match  # => ""
  #     scanner.post_match # => "1975 14:39"
  #     ```
  #
  # - `specifier` positive integer. returns the `n`th capture, or `nil` if out of range:
  #
  #     ```rb
  #     scanner[1] # => "Fri"
  #     scanner[2] # => "Dec"
  #     scanner[3] # => "12"
  #     scanner[4] # => nil
  #     ```
  #
  # - `specifier` negative integer. counts backward from the last subgroup:
  #
  #     ```rb
  #     scanner[-1] # => "12"
  #     scanner[-4] # => "Fri Dec 12 "
  #     scanner[-5] # => nil
  #     ```
  #
  # - `specifier` symbol or string. returns the named subgroup, or `nil` if no such:
  #
  #     ```rb
  #     scanner[:wday]  # => "Fri"
  #     scanner['wday'] # => "Fri"
  #     scanner[:month] # => "Dec"
  #     scanner[:day]   # => "12"
  #     scanner[:nope]  # => nil
  #     ```
  #
  # When there are no captures, only `[0]` returns non-`nil`:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.exist?(/bar/)
  # scanner[0] # => "bar"
  # scanner[1] # => nil
  # ```
  #
  # For a failed match, even `[0]` returns `nil`:
  #
  # ```rb
  # scanner.scan(/nope/) # => nil
  # scanner[0]           # => nil
  # scanner[1]           # => nil
  # ```
  def [](p1) end

  # Returns whether the [position][2] is at the beginning of a line;
  # that is, at the beginning of the [stored string][1]
  # or immediately after a newline:
  #
  #     scanner = StringScanner.new(MULTILINE_TEXT)
  #     scanner.string
  #     # => "Go placidly amid the noise and haste,\nand remember what peace there may be in silence.\n"
  #     scanner.pos                # => 0
  #     scanner.beginning_of_line? # => true
  #
  #     scanner.scan_until(/,/)    # => "Go placidly amid the noise and haste,"
  #     scanner.beginning_of_line? # => false
  #
  #     scanner.scan(/\n/)         # => "\n"
  #     scanner.beginning_of_line? # => true
  #
  #     scanner.terminate
  #     scanner.beginning_of_line? # => true
  #
  #     scanner.concat('x')
  #     scanner.terminate
  #     scanner.beginning_of_line? # => false
  #
  # StringScanner#bol? is an alias for StringScanner#beginning_of_line?.
  def beginning_of_line?; end

  # Returns the array of [captured match values][13] at indexes `(1..)`
  # if the most recent match attempt succeeded, or `nil` otherwise:
  #
  # ```rb
  # scanner = StringScanner.new('Fri Dec 12 1975 14:39')
  # scanner.captures         # => nil
  #
  # scanner.exist?(/(?<wday>\w+) (?<month>\w+) (?<day>\d+) /)
  # scanner.captures         # => ["Fri", "Dec", "12"]
  # scanner.values_at(*0..4) # => ["Fri Dec 12 ", "Fri", "Dec", "12", nil]
  #
  # scanner.exist?(/Fri/)
  # scanner.captures         # => []
  #
  # scanner.scan(/nope/)
  # scanner.captures         # => nil
  # ```
  def captures; end

  def charpos; end

  # Attempts to [match][17] the given `pattern`
  # at the beginning of the [target substring][3];
  # does not modify the [positions][11].
  #
  # If the match succeeds:
  #
  # - Returns the matched substring.
  # - Sets all [match values][9].
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.pos = 3
  # scanner.check('bar') # => "bar"
  # put_match_values(scanner)
  # # Basic match values:
  # #   matched?:       true
  # #   matched_size:   3
  # #   pre_match:      "foo"
  # #   matched  :      "bar"
  # #   post_match:     "baz"
  # # Captured match values:
  # #   size:           1
  # #   captures:       []
  # #   named_captures: {}
  # #   values_at:      ["bar", nil]
  # #   []:
  # #     [0]:          "bar"
  # #     [1]:          nil
  # # => 0..1
  # put_situation(scanner)
  # # Situation:
  # #   pos:       3
  # #   charpos:   3
  # #   rest:      "barbaz"
  # #   rest_size: 6
  # ```
  #
  # If the match fails:
  #
  # - Returns `nil`.
  # - Clears all [match values][9].
  #
  # ```rb
  # scanner.check(/nope/)          # => nil
  # match_values_cleared?(scanner) # => true
  # ```
  def check(pattern) end

  # Attempts to [match][17] the given `pattern`
  # anywhere (at any [position][2])
  # in the [target substring][3];
  # does not modify the [positions][11].
  #
  # If the match succeeds:
  #
  # - Sets all [match values][9].
  # - Returns the matched substring,
  #   which extends from the current [position][2]
  #   to the end of the matched substring.
  #
  # ```rb
  # scanner = StringScanner.new('foobarbazbatbam')
  # scanner.pos = 6
  # scanner.check_until(/bat/) # => "bazbat"
  # put_match_values(scanner)
  # # Basic match values:
  # #   matched?:       true
  # #   matched_size:   3
  # #   pre_match:      "foobarbaz"
  # #   matched  :      "bat"
  # #   post_match:     "bam"
  # # Captured match values:
  # #   size:           1
  # #   captures:       []
  # #   named_captures: {}
  # #   values_at:      ["bat", nil]
  # #   []:
  # #     [0]:          "bat"
  # #     [1]:          nil
  # put_situation(scanner)
  # # Situation:
  # #   pos:       6
  # #   charpos:   6
  # #   rest:      "bazbatbam"
  # #   rest_size: 9
  # ```
  #
  # If the match fails:
  #
  # - Clears all [match values][9].
  # - Returns `nil`.
  #
  # ```rb
  # scanner.check_until(/nope/)    # => nil
  # match_values_cleared?(scanner) # => true
  # ```
  def check_until(pattern) end

  # - Appends the given `more_string`
  #   to the [stored string][1].
  # - Returns `self`.
  # - Does not affect the [positions][11]
  #   or [match values][9].
  #
  # ```rb
  # scanner = StringScanner.new('foo')
  # scanner.string           # => "foo"
  # scanner.terminate
  # scanner.concat('barbaz') # => #<StringScanner 3/9 "foo" @ "barba...">
  # scanner.string           # => "foobarbaz"
  # put_situation(scanner)
  # # Situation:
  # #   pos:       3
  # #   charpos:   3
  # #   rest:      "barbaz"
  # #   rest_size: 6
  # ```
  def concat(more_string) end
  alias << concat

  # Returns whether the [position][2]
  # is at the end of the [stored string][1]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.eos? # => false
  # pos = 3
  # scanner.eos? # => false
  # scanner.terminate
  # scanner.eos? # => true
  # ```
  def eos?; end

  # Attempts to [match][17] the given `pattern`
  # anywhere (at any [position][2])
  # n the [target substring][3];
  # does not modify the [positions][11].
  #
  # If the match succeeds:
  #
  # - Returns a byte offset:
  #   the distance in bytes between the current [position][2]
  #   and the end of the matched substring.
  # - Sets all [match values][9].
  #
  # ```rb
  # scanner = StringScanner.new('foobarbazbatbam')
  # scanner.pos = 6
  # scanner.exist?(/bat/) # => 6
  # put_match_values(scanner)
  # # Basic match values:
  # #   matched?:       true
  # #   matched_size:   3
  # #   pre_match:      "foobarbaz"
  # #   matched  :      "bat"
  # #   post_match:     "bam"
  # # Captured match values:
  # #   size:           1
  # #   captures:       []
  # #   named_captures: {}
  # #   values_at:      ["bat", nil]
  # #   []:
  # #     [0]:          "bat"
  # #     [1]:          nil
  # put_situation(scanner)
  # # Situation:
  # #   pos:       6
  # #   charpos:   6
  # #   rest:      "bazbatbam"
  # #   rest_size: 9
  # ```
  #
  # If the match fails:
  #
  # - Returns `nil`.
  # - Clears all [match values][9].
  #
  # ```rb
  # scanner.exist?(/nope/)         # => nil
  # match_values_cleared?(scanner) # => true
  # ```
  def exist?(pattern) end

  # Returns whether the [fixed-anchor property][10] is set.
  def fixed_anchor?; end

  def get_byte; end

  def getch; end

  # Returns a string representation of `self` that may show:
  #
  # 1. The current [position][2].
  # 2. The size (in bytes) of the [stored string][1].
  # 3. The substring preceding the current position.
  # 4. The substring following the current position (which is also the [target substring][3]).
  #
  # ```rb
  # scanner = StringScanner.new("Fri Dec 12 1975 14:39")
  # scanner.pos = 11
  # scanner.inspect # => "#<StringScanner 11/21 \"...c 12 \" @ \"1975 ...\">"
  # ```
  #
  # If at beginning-of-string, item 4 above (following substring) is omitted:
  #
  # ```rb
  # scanner.reset
  # scanner.inspect # => "#<StringScanner 0/21 @ \"Fri D...\">"
  # ```
  #
  # If at end-of-string, all items above are omitted:
  #
  # ```rb
  # scanner.terminate
  # scanner.inspect # => "#<StringScanner fin>"
  # ```
  def inspect; end

  # Attempts to [match][17] the given `pattern`
  # at the beginning of the [target substring][3];
  # does not modify the [positions][11].
  #
  # If the match succeeds:
  #
  # - Sets [match values][9].
  # - Returns the size in bytes of the matched substring.
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.pos = 3
  # scanner.match?(/bar/) => 3
  # put_match_values(scanner)
  # # Basic match values:
  # #   matched?:       true
  # #   matched_size:   3
  # #   pre_match:      "foo"
  # #   matched  :      "bar"
  # #   post_match:     "baz"
  # # Captured match values:
  # #   size:           1
  # #   captures:       []
  # #   named_captures: {}
  # #   values_at:      ["bar", nil]
  # #   []:
  # #     [0]:          "bar"
  # #     [1]:          nil
  # put_situation(scanner)
  # # Situation:
  # #   pos:       3
  # #   charpos:   3
  # #   rest:      "barbaz"
  # #   rest_size: 6
  # ```
  #
  # If the match fails:
  #
  # - Clears match values.
  # - Returns `nil`.
  # - Does not increment positions.
  #
  # ```rb
  # scanner.match?(/nope/)         # => nil
  # match_values_cleared?(scanner) # => true
  # ```
  def match?(pattern) end

  # Returns the matched substring from the most recent [match][17] attempt
  # if it was successful,
  # or `nil` otherwise;
  # see [Basic Matched Values][18]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.matched        # => nil
  # scanner.pos = 3
  # scanner.match?(/bar/)  # => 3
  # scanner.matched        # => "bar"
  # scanner.match?(/nope/) # => nil
  # scanner.matched        # => nil
  # ```
  def matched; end

  # Returns `true` of the most recent [match attempt][17] was successful,
  # `false` otherwise;
  # see [Basic Matched Values][18]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.matched?       # => false
  # scanner.pos = 3
  # scanner.exist?(/baz/)  # => 6
  # scanner.matched?       # => true
  # scanner.exist?(/nope/) # => nil
  # scanner.matched?       # => false
  # ```
  def matched?; end

  # Returns the size (in bytes) of the matched substring
  # from the most recent match [match attempt][17] if it was successful,
  # or `nil` otherwise;
  # see [Basic Matched Values][18]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.matched_size   # => nil
  #
  # pos = 3
  # scanner.exist?(/baz/)  # => 9
  # scanner.matched_size   # => 3
  #
  # scanner.exist?(/nope/) # => nil
  # scanner.matched_size   # => nil
  # ```
  def matched_size; end

  # Returns the array of captured match values at indexes (1..)
  # if the most recent match attempt succeeded, or nil otherwise;
  # see [Captured Match Values][13]:
  #
  # ```rb
  # scanner = StringScanner.new('Fri Dec 12 1975 14:39')
  # scanner.named_captures # => {}
  #
  # pattern = /(?<wday>\w+) (?<month>\w+) (?<day>\d+) /
  # scanner.match?(pattern)
  # scanner.named_captures # => {"wday"=>"Fri", "month"=>"Dec", "day"=>"12"}
  #
  # scanner.string = 'nope'
  # scanner.match?(pattern)
  # scanner.named_captures # => {"wday"=>nil, "month"=>nil, "day"=>nil}
  #
  # scanner.match?(/nosuch/)
  # scanner.named_captures # => {}
  # ```
  def named_captures; end

  # Returns the substring `string[pos, length]`;
  # does not update [match values][9] or [positions][11]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.pos = 3
  # scanner.peek(3)   # => "bar"
  # scanner.terminate
  # scanner.peek(3)   # => ""
  # ```
  def peek(length) end

  # Peeks at the current byte and returns it as an integer.
  #
  #   s = StringScanner.new('ab')
  #   s.peek_byte         # => 97
  def peek_byte; end

  def pos; end
  alias pointer pos

  def pos=(p1) end
  alias pointer= pos=

  # Returns the substring that follows the matched substring
  # from the most recent match attempt if it was successful,
  # or `nil` otherwise;
  # see [Basic Match Values][18]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.post_match     # => nil
  #
  # scanner.pos = 3
  # scanner.match?(/bar/)  # => 3
  # scanner.post_match     # => "baz"
  #
  # scanner.match?(/nope/) # => nil
  # scanner.post_match     # => nil
  # ```
  def post_match; end

  # Returns the substring that precedes the matched substring
  # from the most recent match attempt if it was successful,
  # or `nil` otherwise;
  # see [Basic Match Values][18]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.pre_match      # => nil
  #
  # scanner.pos = 3
  # scanner.exist?(/baz/)  # => 6
  # scanner.pre_match      # => "foobar" # Substring of entire string, not just target string.
  #
  # scanner.exist?(/nope/) # => nil
  # scanner.pre_match      # => nil
  # ```
  def pre_match; end

  # Sets both [byte position][2] and [character position][7] to zero,
  # and clears [match values][9];
  # returns +self+:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.exist?(/bar/)          # => 6
  # scanner.reset                  # => #<StringScanner 0/9 @ "fooba...">
  # put_situation(scanner)
  # # Situation:
  # #   pos:       0
  # #   charpos:   0
  # #   rest:      "foobarbaz"
  # #   rest_size: 9
  # # => nil
  # match_values_cleared?(scanner) # => true
  # ```
  def reset; end

  # Returns the 'rest' of the [stored string][1] (all after the current [position][2]),
  # which is the [target substring][3]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.rest # => "foobarbaz"
  # scanner.pos = 3
  # scanner.rest # => "barbaz"
  # scanner.terminate
  # scanner.rest # => ""
  # ```
  def rest; end

  # Returns the size (in bytes) of the #rest of the [stored string][1]:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.rest      # => "foobarbaz"
  # scanner.rest_size # => 9
  # scanner.pos = 3
  # scanner.rest      # => "barbaz"
  # scanner.rest_size # => 6
  # scanner.terminate
  # scanner.rest      # => ""
  # scanner.rest_size # => 0
  # ```
  def rest_size; end

  def scan(p1) end

  # Scans one byte and returns it as an integer.
  # This method is not multibyte character sensitive.
  # See also: #getch.
  def scan_byte; end

  def scan_until(p1) end

  # Returns the count of captures if the most recent match attempt succeeded, `nil` otherwise;
  # see [Captures Match Values][13]:
  #
  # ```rb
  # scanner = StringScanner.new('Fri Dec 12 1975 14:39')
  # scanner.size                        # => nil
  #
  # pattern = /(?<wday>\w+) (?<month>\w+) (?<day>\d+) /
  # scanner.match?(pattern)
  # scanner.values_at(*0..scanner.size) # => ["Fri Dec 12 ", "Fri", "Dec", "12", nil]
  # scanner.size                        # => 4
  #
  # scanner.match?(/nope/)              # => nil
  # scanner.size                        # => nil
  # ```
  def size; end

  def skip(p1) end

  def skip_until(p1) end

  # Returns the [stored string][1]:
  #
  # ```rb
  # scanner = StringScanner.new('foobar')
  # scanner.string # => "foobar"
  # scanner.concat('baz')
  # scanner.string # => "foobarbaz"
  # ```
  def string; end

  # Replaces the [stored string][1] with the given `other_string`:
  #
  # - Sets both [positions][11] to zero.
  # - Clears [match values][9].
  # - Returns `other_string`.
  #
  # ```rb
  # scanner = StringScanner.new('foobar')
  # scanner.scan(/foo/)
  # put_situation(scanner)
  # # Situation:
  # #   pos:       3
  # #   charpos:   3
  # #   rest:      "bar"
  # #   rest_size: 3
  # match_values_cleared?(scanner) # => false
  #
  # scanner.string = 'baz'         # => "baz"
  # put_situation(scanner)
  # # Situation:
  # #   pos:       0
  # #   charpos:   0
  # #   rest:      "baz"
  # #   rest_size: 3
  # match_values_cleared?(scanner) # => true
  # ```
  def string=(other_string) end

  def terminate; end

  # Sets the [position][2] to its value previous to the recent successful
  # [match][17] attempt:
  #
  # ```rb
  # scanner = StringScanner.new('foobarbaz')
  # scanner.scan(/foo/)
  # put_situation(scanner)
  # # Situation:
  # #   pos:       3
  # #   charpos:   3
  # #   rest:      "barbaz"
  # #   rest_size: 6
  # scanner.unscan
  # # => #<StringScanner 0/9 @ "fooba...">
  # put_situation(scanner)
  # # Situation:
  # #   pos:       0
  # #   charpos:   0
  # #   rest:      "foobarbaz"
  # #   rest_size: 9
  # ```
  #
  # Raises an exception if match values are clear:
  #
  # ```rb
  # scanner.scan(/nope/)           # => nil
  # match_values_cleared?(scanner) # => true
  # scanner.unscan                 # Raises StringScanner::Error.
  # ```
  def unscan; end

  # Returns an array of captured substrings, or `nil` of none.
  #
  # For each `specifier`, the returned substring is `[specifier]`;
  # see #[].
  #
  # ```rb
  # scanner = StringScanner.new('Fri Dec 12 1975 14:39')
  # pattern = /(?<wday>\w+) (?<month>\w+) (?<day>\d+) /
  # scanner.match?(pattern)
  # scanner.values_at(*0..3)               # => ["Fri Dec 12 ", "Fri", "Dec", "12"]
  # scanner.values_at(*%i[wday month day]) # => ["Fri", "Dec", "12"]
  # ```
  def values_at(*indexes) end

  private

  # Returns a shallow copy of `self`;
  # the [stored string][1] in the copy is the same string as in `self`.
  def initialize_copy(p1) end

  def scan_base10_integer; end

  def scan_base16_integer; end

  class Error < StandardError
  end
end
