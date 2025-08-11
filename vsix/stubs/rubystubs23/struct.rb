# frozen_string_literal: true

# A Struct is a convenient way to bundle a number of attributes together,
# using accessor methods, without having to write an explicit class.
#
# The Struct class generates new subclasses that hold a set of members and
# their values.  For each member a reader and writer method is created
# similar to Module#attr_accessor.
#
#    Customer = Struct.new(:name, :address) do
#      def greeting
#        "Hello #{name}!"
#      end
#    end
#
#    dave = Customer.new("Dave", "123 Main")
#    dave.name     #=> "Dave"
#    dave.greeting #=> "Hello Dave!"
#
# See Struct::new for further examples of creating struct subclasses and
# instances.
#
# In the method descriptions that follow a "member" parameter refers to a
# struct member which is either a quoted string (<code>"name"</code>) or a
# Symbol (<code>:name</code>).
class Struct
  include Enumerable

  # Group
  #
  # Group is a Struct that is only available when compiled with +HAVE_GETGRENT+.
  #
  # The struct contains the following members:
  #
  # name::
  #      contains the name of the group as a String.
  # passwd::
  #      contains the encrypted password as a String. An 'x' is
  #      returned if password access to the group is not available; an empty
  #      string is returned if no password is needed to obtain membership of
  #      the group.
  #
  #      Must be compiled with +HAVE_STRUCT_GROUP_GR_PASSWD+.
  # gid::
  #      contains the group's numeric ID as an integer.
  # mem::
  #      is an Array of Strings containing the short login names of the
  #      members of the group.
  Group = _
  # Passwd
  #
  # Passwd is a Struct that contains the following members:
  #
  # name::
  #      contains the short login name of the user as a String.
  # passwd::
  #      contains the encrypted password of the user as a String.
  #      an 'x' is returned if shadow passwords are in use. An '*' is returned
  #      if the user cannot log in using a password.
  # uid::
  #      contains the integer user ID (uid) of the user.
  # gid::
  #      contains the integer group ID (gid) of the user's primary group.
  # dir::
  #      contains the path to the home directory of the user as a String.
  # shell::
  #      contains the path to the login shell of the user as a String.
  #
  # === The following members below are optional, and must be compiled with special flags:
  #
  # gecos::
  #     contains a longer String description of the user, such as
  #     a full name. Some Unix systems provide structured information in the
  #     gecos field, but this is system-dependent.
  #     must be compiled with +HAVE_STRUCT_PASSWD_PW_GECOS+
  # change::
  #     password change time(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_CHANGE+
  # quota::
  #     quota value(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_QUOTA+
  # age::
  #     password age(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_AGE+
  # class::
  #     user access class(string) must be compiled with +HAVE_STRUCT_PASSWD_PW_CLASS+
  # comment::
  #     comment(string) must be compiled with +HAVE_STRUCT_PASSWD_PW_COMMENT+
  # expire::
  #      account expiration time(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_EXPIRE+
  Passwd = _
  Tms = _

  # The first two forms are used to create a new Struct subclass +class_name+
  # that can contain a value for each +member_name+.  This subclass can be
  # used to create instances of the structure like any other Class.
  #
  # If the +class_name+ is omitted an anonymous structure class will be
  # created.  Otherwise, the name of this struct will appear as a constant in
  # class Struct, so it must be unique for all Structs in the system and
  # must start with a capital letter.  Assigning a structure class to a
  # constant also gives the class the name of the constant.
  #
  #    # Create a structure with a name under Struct
  #    Struct.new("Customer", :name, :address)
  #    #=> Struct::Customer
  #    Struct::Customer.new("Dave", "123 Main")
  #    #=> #<struct Struct::Customer name="Dave", address="123 Main">
  #
  # If a block is given it will be evaluated in the context of
  # +StructClass+, passing the created class as a parameter:
  #
  #    Customer = Struct.new(:name, :address) do
  #      def greeting
  #        "Hello #{name}!"
  #      end
  #    end
  #    Customer.new("Dave", "123 Main").greeting  # => "Hello Dave!"
  #
  # This is the recommended way to customize a struct.  Subclassing an
  # anonymous struct creates an extra anonymous class that will never be used.
  #
  # The last two forms create a new instance of a struct subclass.  The number
  # of +value+ parameters must be less than or equal to the number of
  # attributes defined for the structure.  Unset parameters default to +nil+.
  # Passing more parameters than number of attributes will raise
  # an ArgumentError.
  #
  #    # Create a structure named by its constant
  #    Customer = Struct.new(:name, :address)
  #    #=> Customer
  #    Customer.new("Dave", "123 Main")
  #    #=> #<struct Customer name="Dave", address="123 Main">
  def initialize(*several_variants) end

  # Equality---Returns +true+ if +other+ has the same struct subclass and has
  # equal member values (according to Object#==).
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe   = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joejr = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    jane  = Customer.new("Jane Doe", "456 Elm, Anytown NC", 12345)
  #    joe == joejr   #=> true
  #    joe == jane    #=> false
  def ==(other) end

  # Attribute Reference---Returns the value of the given struct +member+ or
  # the member at the given +index+.   Raises NameError if the +member+ does
  # not exist and IndexError if the +index+ is out of range.
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #
  #    joe["name"]   #=> "Joe Smith"
  #    joe[:name]    #=> "Joe Smith"
  #    joe[0]        #=> "Joe Smith"
  def [](*several_variants) end

  # Attribute Assignment---Sets the value of the given struct +member+ or
  # the member at the given +index+.  Raises NameError if the +member+ does not
  # exist and IndexError if the +index+ is out of range.
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #
  #    joe["name"] = "Luke"
  #    joe[:zip]   = "90210"
  #
  #    joe.name   #=> "Luke"
  #    joe.zip    #=> "90210"
  def []=(*several_variants) end

  # Extracts the nested value specified by the sequence of +key+
  # objects by calling +dig+ at each step, returning +nil+ if any
  # intermediate step is +nil+.
  #
  #   klass = Struct.new(:a)
  #   o = klass.new(klass.new({b: [1, 2, 3]}))
  #
  #   o.dig(:a, :a, :b, 0)   #=> 1
  #   o.dig(:b, 0)           #=> nil
  def dig(key, *args) end

  # Yields the value of each struct member in order.  If no block is given an
  # enumerator is returned.
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joe.each {|x| puts(x) }
  #
  # Produces:
  #
  #    Joe Smith
  #    123 Maple, Anytown NC
  #    12345
  def each; end

  # Yields the name and value of each struct member in order.  If no block is
  # given an enumerator is returned.
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joe.each_pair {|name, value| puts("#{name} => #{value}") }
  #
  # Produces:
  #
  #    name => Joe Smith
  #    address => 123 Maple, Anytown NC
  #    zip => 12345
  def each_pair; end

  # Hash equality---+other+ and +struct+ refer to the same hash key if they
  # have the same struct subclass and have equal member values (according to
  # Object#eql?).
  def eql?(other) end

  # Returns a hash value based on this struct's contents (see Object#hash).
  #
  # See also Object#hash.
  def hash; end

  # Describe the contents of this struct in a string.
  def inspect; end
  alias to_s inspect

  # Returns the struct members as an array of symbols:
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joe.members   #=> [:name, :address, :zip]
  def members; end

  # Yields each member value from the struct to the block and returns an Array
  # containing the member values from the +struct+ for which the given block
  # returns a true value (equivalent to Enumerable#select).
  #
  #    Lots = Struct.new(:a, :b, :c, :d, :e, :f)
  #    l = Lots.new(11, 22, 33, 44, 55, 66)
  #    l.select {|v| (v % 2).zero? }   #=> [22, 44, 66]
  def select; end

  # Returns the number of struct members.
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joe.length   #=> 3
  def size; end
  alias length size

  # Returns the values for this struct as an Array.
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joe.to_a[1]   #=> "123 Maple, Anytown NC"
  def to_a; end
  alias values to_a

  # Returns a Hash containing the names and values for the struct's members.
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joe.to_h[:address]   #=> "123 Maple, Anytown NC"
  def to_h; end

  # Returns the struct member values for each +selector+ as an Array.  A
  # +selector+ may be either an Integer offset or a Range of offsets (as in
  # Array#values_at).
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #    joe.values_at 0, 2 #=> ["Joe Smith", 12345]
  def values_at(selector, *args) end
end
