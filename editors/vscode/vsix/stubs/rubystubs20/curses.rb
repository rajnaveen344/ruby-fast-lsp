# frozen_string_literal: true

# == Description
# An implementation of the CRT screen handling and optimization library.
#
# == Structures and such
#
# === Classes
#
# * Curses::Window - class with the means to draw a window or box
# * Curses::MouseEvent - class for collecting mouse events
#
# === Modules
#
# Curses:: The curses implementation
# Curses::Key:: Collection of constants for keypress events
#
# == Examples
#
# * hello.rb
#     require "curses"
#     include Curses
#
#     def show_message(message)
#       width = message.length + 6
#       win = Window.new(5, width,
#                    (lines - 5) / 2, (cols - width) / 2)
#       win.box(?|, ?-)
#       win.setpos(2, 3)
#       win.addstr(message)
#       win.refresh
#       win.getch
#       win.close
#     end
#
#     init_screen
#     begin
#       crmode
#     #  show_message("Hit any key")
#       setpos((lines - 5) / 2, (cols - 10) / 2)
#       addstr("Hit any key")
#       refresh
#       getch
#       show_message("Hello, World!")
#       refresh
#     ensure
#       close_screen
#     end
#
# * rain.rb
#     # rain for a curses test
#
#     require "curses"
#     include Curses
#
#     def onsig(sig)
#       close_screen
#       exit sig
#     end
#
#     def ranf
#       rand(32767).to_f / 32767
#     end
#
#     # main #
#     for i in %w[HUP INT QUIT TERM]
#       if trap(i, "SIG_IGN") != 0 then  # 0 for SIG_IGN
#         trap(i) {|sig| onsig(sig) }
#       end
#     end
#
#     init_screen
#     nl
#     noecho
#     srand
#
#     xpos = {}
#     ypos = {}
#     r = lines - 4
#     c = cols - 4
#     for i in 0 .. 4
#       xpos[i] = (c * ranf).to_i + 2
#       ypos[i] = (r * ranf).to_i + 2
#     end
#
#     i = 0
#     while TRUE
#       x = (c * ranf).to_i + 2
#       y = (r * ranf).to_i + 2
#
#
#       setpos(y, x); addstr(".")
#
#       setpos(ypos[i], xpos[i]); addstr("o")
#
#       i = if i == 0 then 4 else i - 1 end
#       setpos(ypos[i], xpos[i]); addstr("O")
#
#       i = if i == 0 then 4 else i - 1 end
#       setpos(ypos[i] - 1, xpos[i]);      addstr("-")
#       setpos(ypos[i],     xpos[i] - 1); addstr("|.|")
#       setpos(ypos[i] + 1, xpos[i]);      addstr("-")
#
#       i = if i == 0 then 4 else i - 1 end
#       setpos(ypos[i] - 2, xpos[i]);       addstr("-")
#       setpos(ypos[i] - 1, xpos[i] - 1);  addstr("/ \\")
#       setpos(ypos[i],     xpos[i] - 2); addstr("| O |")
#       setpos(ypos[i] + 1, xpos[i] - 1); addstr("\\ /")
#       setpos(ypos[i] + 2, xpos[i]);       addstr("-")
#
#       i = if i == 0 then 4 else i - 1 end
#       setpos(ypos[i] - 2, xpos[i]);       addstr(" ")
#       setpos(ypos[i] - 1, xpos[i] - 1);  addstr("   ")
#       setpos(ypos[i],     xpos[i] - 2); addstr("     ")
#       setpos(ypos[i] + 1, xpos[i] - 1);  addstr("   ")
#       setpos(ypos[i] + 2, xpos[i]);       addstr(" ")
#
#
#       xpos[i] = x
#       ypos[i] = y
#       refresh
#       sleep(0.5)
#     end
#
#     # end of main
module Curses
  # ALL_MOUSE_EVENTS
  #
  # Mouse event mask:
  # report all button state changes
  #
  # See Curses.getmouse
  ALL_MOUSE_EVENTS = _
  # A_ALTCHARSET
  #
  # Attribute mask:
  # Alternate character set
  #
  # See Curses.attrset
  A_ALTCHARSET = _
  # A_ATTRIBUTES
  #
  # Character attribute mask:
  # Bit-mask to extract attributes
  #
  # See Curses.inch or Curses::Window.inch
  A_ATTRIBUTES = _
  # A_BLINK
  #
  # Attribute mask:
  # Blinking
  #
  # See Curses.attrset
  A_BLINK = _
  # A_BOLD
  #
  # Attribute mask:
  # Extra bright or bold
  #
  # See Curses.attrset
  A_BOLD = _
  # A_CHARTEXT
  #
  # Attribute mask:
  # Bit-mask to extract a character
  #
  # See Curses.attrset
  A_CHARTEXT = _
  # A_COLOR
  #
  # Character attribute mask:
  # Bit-mask to extract color-pair field information
  #
  # See Curses.inch or Curses::Window.inch
  A_COLOR = _
  # A_DIM
  #
  # Attribute mask:
  # Half bright
  #
  # See Curses.attrset
  A_DIM = _
  # A_HORIZONTAL
  #
  # Attribute mask:
  # horizontal highlight
  #
  # Check system curs_attr(3x) for support
  A_HORIZONTAL = _
  # A_INVIS
  #
  # Attribute mask:
  # Invisible or blank mode
  #
  # See Curses.attrset
  A_INVIS = _
  # A_LEFT
  #
  # Attribute mask:
  # left highlight
  #
  # Check system curs_attr(3x) for support
  A_LEFT = _
  # A_LOW
  #
  # Attribute mask:
  # low highlight
  #
  # Check system curs_attr(3x) for support
  A_LOW = _
  # A_NORMAL
  #
  # Attribute mask:
  # Normal display (no highlight)
  #
  # See Curses.attrset
  A_NORMAL = _
  # A_PROTECT
  #
  # Attribute mask:
  # Protected mode
  #
  # See Curses.attrset
  A_PROTECT = _
  # A_REVERSE
  #
  # Attribute mask:
  # Reverse video
  #
  # See Curses.attrset
  A_REVERSE = _
  # A_RIGHT
  #
  # Attribute mask:
  # right highlight
  #
  # Check system curs_attr(3x) for support
  A_RIGHT = _
  # A_STANDOUT
  #
  # Attribute mask:
  # Best highlighting mode of the terminal.
  #
  # See Curses.attrset
  A_STANDOUT = _
  # A_TOP
  #
  # Attribute mask:
  # top highlight
  #
  # Check system curs_attr(3x) for support
  A_TOP = _
  # A_UNDERLINE
  #
  # Attribute mask:
  # Underlining
  #
  # See Curses.attrset
  A_UNDERLINE = _
  # A_VERTICAL
  #
  # Attribute mask:
  # vertical highlight
  #
  # Check system curs_attr(3x) for support
  A_VERTICAL = _
  # BUTTON1_CLICKED
  #
  # Mouse event mask:
  # mouse button 1 clicked
  #
  # See Curses.getmouse
  BUTTON1_CLICKED = _
  # BUTTON1_DOUBLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 1 double clicked
  #
  # See Curses.getmouse
  BUTTON1_DOUBLE_CLICKED = _
  # BUTTON1_PRESSED
  #
  # Mouse event mask:
  # mouse button 1 down
  #
  # See Curses.getmouse
  BUTTON1_PRESSED = _
  # BUTTON1_RELEASED
  #
  # Mouse event mask:
  # mouse button 1 up
  #
  # See Curses.getmouse
  BUTTON1_RELEASED = _
  # BUTTON1_TRIPLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 1 triple clicked
  #
  # See Curses.getmouse
  BUTTON1_TRIPLE_CLICKED = _
  # BUTTON2_CLICKED
  #
  # Mouse event mask:
  # mouse button 2 clicked
  #
  # See Curses.getmouse
  BUTTON2_CLICKED = _
  # BUTTON2_DOUBLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 2 double clicked
  #
  # See Curses.getmouse
  BUTTON2_DOUBLE_CLICKED = _
  # BUTTON2_PRESSED
  #
  # Mouse event mask:
  # mouse button 2 down
  #
  # See Curses.getmouse
  BUTTON2_PRESSED = _
  # BUTTON2_RELEASED
  #
  # Mouse event mask:
  # mouse button 2 up
  #
  # See Curses.getmouse
  BUTTON2_RELEASED = _
  # BUTTON2_TRIPLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 2 triple clicked
  #
  # See Curses.getmouse
  BUTTON2_TRIPLE_CLICKED = _
  # BUTTON3_CLICKED
  #
  # Mouse event mask:
  # mouse button 3 clicked
  #
  # See Curses.getmouse
  BUTTON3_CLICKED = _
  # BUTTON3_DOUBLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 3 double clicked
  #
  # See Curses.getmouse
  BUTTON3_DOUBLE_CLICKED = _
  # BUTTON3_PRESSED
  #
  # Mouse event mask:
  # mouse button 3 down
  #
  # See Curses.getmouse
  BUTTON3_PRESSED = _
  # BUTTON3_RELEASED
  #
  # Mouse event mask:
  # mouse button 3 up
  #
  # See Curses.getmouse
  BUTTON3_RELEASED = _
  # BUTTON3_TRIPLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 3 triple clicked
  #
  # See Curses.getmouse
  BUTTON3_TRIPLE_CLICKED = _
  # BUTTON4_CLICKED
  #
  # Mouse event mask:
  # mouse button 4 clicked
  #
  # See Curses.getmouse
  BUTTON4_CLICKED = _
  # BUTTON4_DOUBLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 4 double clicked
  #
  # See Curses.getmouse
  BUTTON4_DOUBLE_CLICKED = _
  # BUTTON4_PRESSED
  #
  # Mouse event mask:
  # mouse button 4 down
  #
  # See Curses.getmouse
  BUTTON4_PRESSED = _
  # BUTTON4_RELEASED
  #
  # Mouse event mask:
  # mouse button 4 up
  #
  # See Curses.getmouse
  BUTTON4_RELEASED = _
  # BUTTON4_TRIPLE_CLICKED
  #
  # Mouse event mask:
  # mouse button 4 triple clicked
  #
  # See Curses.getmouse
  BUTTON4_TRIPLE_CLICKED = _
  # BUTTON_ALT
  #
  # Mouse event mask:
  # alt was down during button state change
  #
  # See Curses.getmouse
  BUTTON_ALT = _
  # BUTTON_CTRL
  #
  # Mouse event mask:
  # control was down during button state change
  #
  # See Curses.getmouse
  BUTTON_CTRL = _
  # BUTTON_SHIFT
  #
  # Mouse event mask:
  # shift was down during button state change
  #
  # See Curses.getmouse
  BUTTON_SHIFT = _
  # Curses::COLORS
  #
  # Number of the colors available
  COLORS = _
  # Curses::COLOR_BLACK
  #
  # Value of the color black
  COLOR_BLACK = _
  # COLOR_BLUE
  #
  # Value of the color blue
  COLOR_BLUE = _
  # COLOR_CYAN
  #
  # Value of the color cyan
  COLOR_CYAN = _
  # COLOR_GREEN
  #
  # Value of the color green
  COLOR_GREEN = _
  # COLOR_MAGENTA
  #
  # Value of the color magenta
  COLOR_MAGENTA = _
  # COLOR_RED
  #
  # Value of the color red
  COLOR_RED = _
  # COLOR_WHITE
  #
  # Value of the color white
  COLOR_WHITE = _
  # COLOR_YELLOW
  #
  # Value of the color yellow
  COLOR_YELLOW = _
  # A1
  # Upper left of keypad
  KEY_A1 = _
  # A3
  # Upper right of keypad
  KEY_A3 = _
  # B2
  # Center of keypad
  KEY_B2 = _
  # BACKSPACE
  # Backspace
  KEY_BACKSPACE = _
  # BEG
  # Beginning key
  KEY_BEG = _
  # BREAK
  # Break key
  KEY_BREAK = _
  # KEY_BTAB
  # Back tab key
  KEY_BTAB = _
  # C1
  # Lower left of keypad
  KEY_C1 = _
  # C3
  # Lower right of keypad
  KEY_C3 = _
  # CANCEL
  # Cancel key
  KEY_CANCEL = _
  # CATAB
  # Clear all tabs
  KEY_CATAB = _
  # CLEAR
  # Clear Screen
  KEY_CLEAR = _
  # CLOSE
  # Close key
  KEY_CLOSE = _
  # COMMAND
  # Cmd (command) key
  KEY_COMMAND = _
  # COPY
  # Copy key
  KEY_COPY = _
  # CREATE
  # Create key
  KEY_CREATE = _
  # CTAB
  # Clear tab
  KEY_CTAB = _
  # DC
  # Delete character
  KEY_DC = _
  # DL
  # Delete line
  KEY_DL = _
  # DOWN
  # the down arrow key
  KEY_DOWN = _
  # EIC
  # Enter insert char mode
  KEY_EIC = _
  # END
  # End key
  KEY_END = _
  # ENTER
  # Enter or send
  KEY_ENTER = _
  # EOL
  # Clear to end of line
  KEY_EOL = _
  # EOS
  # Clear to end of screen
  KEY_EOS = _
  # EXIT
  # Exit key
  KEY_EXIT = _
  # FIND
  # Find key
  KEY_FIND = _
  # HELP
  # Help key
  KEY_HELP = _
  # HOME
  # Home key (upward+left arrow)
  KEY_HOME = _
  # IC
  # Insert char or enter insert mode
  KEY_IC = _
  # IL
  # Insert line
  KEY_IL = _
  # LEFT
  # the left arrow key
  KEY_LEFT = _
  # LL
  # Home down or bottom (lower left)
  KEY_LL = _
  # MARK
  # Mark key
  KEY_MARK = _
  # MAX
  # The maximum allowed curses key value.
  KEY_MAX = _
  # MESSAGE
  # Message key
  KEY_MESSAGE = _
  # MIN
  # The minimum allowed curses key value.
  KEY_MIN = _
  # MOUSE
  # Mouse event read
  KEY_MOUSE = _
  # MOVE
  # Move key
  KEY_MOVE = _
  # NEXT
  # Next object key
  KEY_NEXT = _
  # NPAGE
  # Next page
  KEY_NPAGE = _
  # OPEN
  # Open key
  KEY_OPEN = _
  # OPTIONS
  # Options key
  KEY_OPTIONS = _
  # PPAGE
  # Previous page
  KEY_PPAGE = _
  # PREVIOUS
  # Previous object key
  KEY_PREVIOUS = _
  # PRINT
  # Print or copy
  KEY_PRINT = _
  # REDO
  # Redo key
  KEY_REDO = _
  # REFERENCE
  # Reference key
  KEY_REFERENCE = _
  # REFRESH
  # Refresh key
  KEY_REFRESH = _
  # REPLACE
  # Replace key
  KEY_REPLACE = _
  # RESET
  # Reset or hard reset
  KEY_RESET = _
  # RESIZE
  # Screen Resized
  KEY_RESIZE = _
  # RESTART
  # Restart key
  KEY_RESTART = _
  # RESUME
  # Resume key
  KEY_RESUME = _
  # RIGHT
  # the right arrow key
  KEY_RIGHT = _
  # SAVE
  # Save key
  KEY_SAVE = _
  # SBEG
  # Shifted beginning key
  KEY_SBEG = _
  # SCANCEL
  # Shifted cancel key
  KEY_SCANCEL = _
  # SCOMMAND
  # Shifted command key
  KEY_SCOMMAND = _
  # SCOPY
  # Shifted copy key
  KEY_SCOPY = _
  # SCREATE
  # Shifted create key
  KEY_SCREATE = _
  # SDC
  # Shifted delete char key
  KEY_SDC = _
  # SDL
  # Shifted delete line key
  KEY_SDL = _
  # SELECT
  # Select key
  KEY_SELECT = _
  # SEND
  # Shifted end key
  KEY_SEND = _
  # SEOL
  # Shifted clear line key
  KEY_SEOL = _
  # SEXIT
  # Shifted exit key
  KEY_SEXIT = _
  # SF
  # Scroll 1 line forward
  KEY_SF = _
  # SFIND
  # Shifted find key
  KEY_SFIND = _
  # SHELP
  # Shifted help key
  KEY_SHELP = _
  # SHOME
  # Shifted home key
  KEY_SHOME = _
  # SIC
  # Shifted input key
  KEY_SIC = _
  # SLEFT
  # Shifted left arrow key
  KEY_SLEFT = _
  # SMESSAGE
  # Shifted message key
  KEY_SMESSAGE = _
  # SMOVE
  # Shifted move key
  KEY_SMOVE = _
  # SNEXT
  # Shifted next key
  KEY_SNEXT = _
  # SOPTIONS
  # Shifted options key
  KEY_SOPTIONS = _
  # SPREVIOUS
  # Shifted previous key
  KEY_SPREVIOUS = _
  # SPRINT
  # Shifted print key
  KEY_SPRINT = _
  # SR
  # Scroll 1 line backware (reverse)
  KEY_SR = _
  # SREDO
  # Shifted redo key
  KEY_SREDO = _
  # SREPLACE
  # Shifted replace key
  KEY_SREPLACE = _
  # SRESET
  # Soft (partial) reset
  KEY_SRESET = _
  # SRIGHT
  # Shifted right arrow key
  KEY_SRIGHT = _
  # SRSUME
  # Shifted resume key
  KEY_SRSUME = _
  # SSAVE
  # Shifted save key
  KEY_SSAVE = _
  # SSUSPEND
  # Shifted suspend key
  KEY_SSUSPEND = _
  # STAB
  # Set tab
  KEY_STAB = _
  # SUNDO
  # Shifted undo key
  KEY_SUNDO = _
  # SUSPEND
  # Suspend key
  KEY_SUSPEND = _
  # UNDO
  # Undo key
  KEY_UNDO = _
  # UP
  # the up arrow key
  KEY_UP = _
  # REPORT_MOUSE_POSITION
  #
  # Mouse event mask:
  # report mouse movement
  #
  # See Curses.getmouse
  REPORT_MOUSE_POSITION = _
  # Identifies curses library version.
  #
  # - "ncurses 5.9.20110404"
  # - "PDCurses 3.4 - Public Domain 2008"
  # - "curses (SVR4)" (System V curses)
  # - "curses (unknown)" (The original BSD curses?  NetBSD maybe.)
  VERSION = _

  # Returns the total time, in milliseconds, for which
  # curses will await a character sequence, e.g., a function key
  def self.ESCDELAY; end

  # Sets the ESCDELAY to Integer +value+
  def self.ESCDELAY=(value) end

  # Returns the number of positions in a tab.
  def self.TABSIZE; end

  # Sets the TABSIZE to Integer +value+
  def self.TABSIZE=(value) end

  # Add a character +ch+, with attributes, then advance the cursor.
  #
  # see also the system manual for curs_addch(3)
  def self.addch(ch) end

  # add a string of characters +str+, to the window and advance cursor
  def self.addstr(str) end

  # Turns on the named attributes +attrs+ without affecting any others.
  #
  # See also Curses::Window.attrset for additional information.
  def self.attroff(attrs) end

  # Turns off the named attributes +attrs+
  # without turning any other attributes on or off.
  #
  # See also Curses::Window.attrset for additional information.
  def self.attron(attrs) end

  # Sets the current attributes of the given window to +attrs+.
  #
  # see also Curses::Window.attrset
  def self.attrset(attrs) end

  # Sounds an audible alarm on the terminal, if possible;
  # otherwise it flashes the screen (visual bell).
  #
  # see also Curses.flash
  def self.beep; end

  # Window background manipulation routines.
  #
  # Set the background property of the current
  # and then apply the character Integer +ch+ setting
  # to every character position in that window.
  #
  # see also the system manual for curs_bkgd(3)
  def self.bkgd(ch) end

  # Manipulate the background of the named window
  # with character Integer +ch+
  #
  # The background becomes a property of the character
  # and moves with the character through any scrolling
  # and insert/delete line/character operations.
  #
  # see also the system manual for curs_bkgd(3)
  def self.bkgdset(ch) end

  # Returns +true+ or +false+ depending on whether the terminal can change color attributes
  def self.can_change_color?; end

  # Put the terminal into cbreak mode.
  #
  # Normally, the tty driver buffers typed characters until
  # a newline or carriage return is typed. The Curses.cbreak
  # routine disables line buffering and erase/kill
  # character-processing (interrupt and flow control characters
  # are unaffected), making characters typed by the user
  # immediately available to the program.
  #
  # The Curses.nocbreak routine returns the terminal to normal (cooked) mode.
  #
  # Initially the terminal may or may not be in cbreak mode,
  # as the mode is inherited; therefore, a program should
  # call Curses.cbreak or Curses.nocbreak explicitly.
  # Most interactive programs using curses set the cbreak mode.
  # Note that Curses.cbreak overrides Curses.raw.
  #
  # see also Curses.raw
  def self.cbreak; end

  # Clears every position on the screen completely,
  # so that a subsequent call by Curses.refresh for the screen/window
  # will be repainted from scratch.
  def self.clear; end

  # A program should always call Curses.close_screen before exiting or
  # escaping from curses mode temporarily. This routine
  # restores tty modes, moves the cursor to the lower
  # left-hand corner of the screen and resets the terminal
  # into the proper non-visual mode.
  #
  # Calling Curses.refresh or Curses.doupdate after a temporary
  # escape causes the program to resume visual mode.
  def self.close_screen; end

  # Returns +true+ if the window/screen has been closed,
  # without any subsequent Curses.refresh calls,
  # returns +false+ otherwise.
  def self.closed?; end

  # Clears to the end of line, that the cursor is currently on.
  def self.clrtoeol; end

  # Returns an 3 item Array of the RGB values in +color+
  def self.color_content(color) end

  # Sets the color pair attributes to +attrs+.
  #
  # This should be equivalent to Curses.attrset(COLOR_PAIR(+attrs+))
  #
  # TODO: validate that equivalency
  def self.color_pair(attrs) end

  # Returns the COLOR_PAIRS available, if the curses library supports it.
  def self.color_pairs; end

  # returns COLORS
  def self.colors; end

  # Returns the number of columns on the screen
  def self.cols; end

  # Put the terminal into cbreak mode.
  #
  # Normally, the tty driver buffers typed characters until
  # a newline or carriage return is typed. The Curses.cbreak
  # routine disables line buffering and erase/kill
  # character-processing (interrupt and flow control characters
  # are unaffected), making characters typed by the user
  # immediately available to the program.
  #
  # The Curses.nocbreak routine returns the terminal to normal (cooked) mode.
  #
  # Initially the terminal may or may not be in cbreak mode,
  # as the mode is inherited; therefore, a program should
  # call Curses.cbreak or Curses.nocbreak explicitly.
  # Most interactive programs using curses set the cbreak mode.
  # Note that Curses.cbreak overrides Curses.raw.
  #
  # see also Curses.raw
  def self.crmode; end

  # Sets Cursor Visibility.
  # 0: invisible
  # 1: visible
  # 2: very visible
  def self.curs_set(visibility) end

  # Save the current terminal modes as the "program"
  # state for use by the Curses.reset_prog_mode
  #
  # This is done automatically by Curses.init_screen
  def self.def_prog_mode; end

  # Delete the character under the cursor
  def self.delch; end

  # Delete the line under the cursor.
  def self.deleteln; end

  # Refreshes the windows and lines.
  #
  # Curses.doupdate allows multiple updates with
  # more efficiency than Curses.refresh alone.
  def self.doupdate; end

  # Enables characters typed by the user
  # to be echoed by Curses.getch as they are typed.
  def self.echo; end

  # Flashs the screen, for visual alarm on the terminal, if possible;
  # otherwise it sounds the alert.
  #
  # see also Curses.beep
  def self.flash; end

  # Read and returns a character from the window.
  #
  # See Curses::Key to all the function KEY_* available
  def self.getch; end

  # Returns coordinates of the mouse.
  #
  # This will read and pop the mouse event data off the queue
  #
  # See the BUTTON*, ALL_MOUSE_EVENTS and REPORT_MOUSE_POSITION constants, to examine the mask of the event
  def self.getmouse; end

  # This is equivalent to a series f Curses::Window.getch calls
  def self.getstr; end

  # Returns +true+ or +false+ depending on whether the terminal has color capbilities.
  def self.has_colors?; end

  # Returns the character at the current position.
  def self.inch; end

  # Changes the definition of a color. It takes four arguments:
  # * the number of the color to be changed, +color+
  # * the amount of red, +r+
  # * the amount of green, +g+
  # * the amount of blue, +b+
  #
  # The value of the first argument must be between 0 and  COLORS.
  # (See the section Colors for the default color index.)  Each
  # of the last three arguments must be a value between 0 and 1000.
  # When Curses.init_color is used, all occurrences of that color
  # on the screen immediately change to the new definition.
  def self.init_color(color, r, g, b) end

  # Changes the definition of a color-pair.
  #
  # It takes three arguments: the number of the color-pair to be changed +pair+,
  # the foreground color number +f+, and the background color number +b+.
  #
  # If the color-pair was previously initialized, the screen is
  # refreshed and all occurrences of that color-pair are changed
  # to the new definition.
  def self.init_pair(pair, f, b) end

  # Initialize a standard screen
  #
  # see also Curses.stdscr
  def self.init_screen; end

  # Insert a character +ch+, before the cursor.
  def self.insch(ch) end

  # Inserts a line above the cursor, and the bottom line is lost
  def self.insertln; end

  # Returns the character string corresponding to key +c+
  def self.keyname(c) end

  # Returns the number of lines on the screen
  def self.lines; end

  # The Curses.mouseinterval function sets the maximum time
  # (in thousands of a second) that can elapse between press
  # and release events for them to be recognized as a click.
  #
  # Use Curses.mouseinterval(0) to disable click resolution.
  # This function returns the previous interval value.
  #
  # Use Curses.mouseinterval(-1) to obtain the interval without
  # altering it.
  #
  # The default is one sixth of a second.
  def self.mouseinterval(interval) end

  # Returns the +mask+ of the reportable events
  def self.mousemask(mask) end

  # Enable the underlying display device to translate
  # the return key into newline on input, and whether it
  # translates newline into return and line-feed on output
  # (in either case, the call Curses.addch('\n') does the
  # equivalent of return and line feed on the virtual screen).
  #
  # Initially, these translations do occur. If you disable
  # them using Curses.nonl, curses will be able to make better use
  # of the line-feed capability, resulting in faster cursor
  # motion. Also, curses will then be able to detect the return key.
  def self.nl; end

  # Put the terminal into normal mode (out of cbreak mode).
  #
  # See Curses.cbreak for more detail.
  def self.nocbreak; end

  # Put the terminal into normal mode (out of cbreak mode).
  #
  # See Curses.cbreak for more detail.
  def self.nocrmode; end

  # Disables characters typed by the user
  # to be echoed by Curses.getch as they are typed.
  def self.noecho; end

  # Disable the underlying display device to translate
  # the return key into newline on input
  #
  # See Curses.nl for more detail
  def self.nonl; end

  # Put the terminal out of raw mode.
  #
  # see Curses.raw for more detail
  def self.noraw; end

  # Returns a 2 item Array, with the foreground and
  # background color, in +pair+
  def self.pair_content(pair) end

  # Returns the Fixnum color pair number of attributes +attrs+.
  def self.pair_number(attrs) end

  # Put the terminal into raw mode.
  #
  # Raw mode is similar to Curses.cbreak mode, in that characters typed
  # are immediately passed through to the user program.
  #
  # The differences are that in raw mode, the interrupt, quit,
  # suspend, and flow control characters are all passed through
  # uninterpreted, instead of generating a signal. The behavior
  # of the BREAK key depends on other bits in the tty driver
  # that are not set by curses.
  def self.raw; end

  # Refreshes the windows and lines.
  def self.refresh; end

  # Reset the current terminal modes to the saved state
  # by the Curses.def_prog_mode
  #
  # This is done automatically by Curses.close_screen
  def self.reset_prog_mode; end

  # Resize the current term to Fixnum +lines+ and Fixnum +cols+
  def self.resize(p1, p2) end

  # Resize the current term to Fixnum +lines+ and Fixnum +cols+
  def self.resizeterm(lines, cols) end

  # Scrolls the current window Fixnum +num+ lines.
  # The current cursor position is not changed.
  #
  # For positive +num+, it scrolls up.
  #
  # For negative +num+, it scrolls down.
  def self.scrl(num) end

  # A setter for the position of the cursor,
  # using coordinates +x+ and +y+
  def self.setpos(y, x) end

  # Set a software scrolling region in a window.
  # +top+ and +bottom+ are lines numbers of the margin.
  #
  # If this option and Curses.scrollok are enabled, an attempt to move off
  # the bottom margin line causes all lines in the scrolling region
  # to scroll one line in the direction of the first line.
  # Only the text of the window is scrolled.
  def self.setscrreg(top, bottom) end

  # Enables the Normal display (no highlight)
  #
  # This is equivalent to Curses.attron(A_NORMAL)
  #
  # see also Curses::Window.attrset for additional information.
  def self.standend; end

  # Enables the best highlighting mode of the terminal.
  #
  # This is equivalent to Curses:Window.attron(A_STANDOUT)
  #
  # see also Curses::Window.attrset additional information
  def self.standout; end

  # Initializes the color attributes, for terminals that support it.
  #
  # This must be called, in order to use color attributes.
  # It is good practice to call it just after Curses.init_screen
  def self.start_color; end

  # The Standard Screen.
  #
  # Upon initializing curses, a default window called stdscr,
  # which is the size of the terminal screen, is created.
  #
  # Many curses functions use this window.
  def self.stdscr; end

  # Sets block and non-blocking reads for the window.
  # - If delay is negative, blocking read is used (i.e., waits indefinitely for input).
  # - If delay is zero, then non-blocking read is used (i.e., read returns ERR if no input is waiting).
  # - If delay is positive, then read blocks for delay milliseconds, and returns ERR if there is still no input.
  def self.timeout=(delay) end

  # Places +ch+ back onto the input queue to be returned by
  # the next call to Curses.getch.
  #
  # There is just one input queue for all windows.
  def self.ungetch(ch) end

  # It pushes a KEY_MOUSE event onto the input queue, and associates with that
  # event the given state data and screen-relative character-cell coordinates.
  #
  # The Curses.ungetmouse function behaves analogously to Curses.ungetch.
  def self.ungetmouse(p1) end

  # tells the curses library to use terminal's default colors.
  #
  # see also the system manual for default_colors(3)
  def self.use_default_colors; end

  private

  # Returns the total time, in milliseconds, for which
  # curses will await a character sequence, e.g., a function key
  def ESCDELAY; end

  # Sets the ESCDELAY to Integer +value+
  def ESCDELAY=(value) end

  # Returns the number of positions in a tab.
  def TABSIZE; end

  # Sets the TABSIZE to Integer +value+
  def TABSIZE=(value) end

  # Add a character +ch+, with attributes, then advance the cursor.
  #
  # see also the system manual for curs_addch(3)
  def addch(ch) end

  # add a string of characters +str+, to the window and advance cursor
  def addstr(str) end

  # Turns on the named attributes +attrs+ without affecting any others.
  #
  # See also Curses::Window.attrset for additional information.
  def attroff(attrs) end

  # Turns off the named attributes +attrs+
  # without turning any other attributes on or off.
  #
  # See also Curses::Window.attrset for additional information.
  def attron(attrs) end

  # Sets the current attributes of the given window to +attrs+.
  #
  # see also Curses::Window.attrset
  def attrset(attrs) end

  # Sounds an audible alarm on the terminal, if possible;
  # otherwise it flashes the screen (visual bell).
  #
  # see also Curses.flash
  def beep; end

  # Window background manipulation routines.
  #
  # Set the background property of the current
  # and then apply the character Integer +ch+ setting
  # to every character position in that window.
  #
  # see also the system manual for curs_bkgd(3)
  def bkgd(ch) end

  # Manipulate the background of the named window
  # with character Integer +ch+
  #
  # The background becomes a property of the character
  # and moves with the character through any scrolling
  # and insert/delete line/character operations.
  #
  # see also the system manual for curs_bkgd(3)
  def bkgdset(ch) end

  # Returns +true+ or +false+ depending on whether the terminal can change color attributes
  def can_change_color?; end

  # Put the terminal into cbreak mode.
  #
  # Normally, the tty driver buffers typed characters until
  # a newline or carriage return is typed. The Curses.cbreak
  # routine disables line buffering and erase/kill
  # character-processing (interrupt and flow control characters
  # are unaffected), making characters typed by the user
  # immediately available to the program.
  #
  # The Curses.nocbreak routine returns the terminal to normal (cooked) mode.
  #
  # Initially the terminal may or may not be in cbreak mode,
  # as the mode is inherited; therefore, a program should
  # call Curses.cbreak or Curses.nocbreak explicitly.
  # Most interactive programs using curses set the cbreak mode.
  # Note that Curses.cbreak overrides Curses.raw.
  #
  # see also Curses.raw
  def cbreak; end
  alias crmode cbreak

  # Clears every position on the screen completely,
  # so that a subsequent call by Curses.refresh for the screen/window
  # will be repainted from scratch.
  def clear; end

  # A program should always call Curses.close_screen before exiting or
  # escaping from curses mode temporarily. This routine
  # restores tty modes, moves the cursor to the lower
  # left-hand corner of the screen and resets the terminal
  # into the proper non-visual mode.
  #
  # Calling Curses.refresh or Curses.doupdate after a temporary
  # escape causes the program to resume visual mode.
  def close_screen; end

  # Returns +true+ if the window/screen has been closed,
  # without any subsequent Curses.refresh calls,
  # returns +false+ otherwise.
  def closed?; end

  # Clears to the end of line, that the cursor is currently on.
  def clrtoeol; end

  # Returns an 3 item Array of the RGB values in +color+
  def color_content(color) end

  # Sets the color pair attributes to +attrs+.
  #
  # This should be equivalent to Curses.attrset(COLOR_PAIR(+attrs+))
  #
  # TODO: validate that equivalency
  def color_pair(attrs) end

  # Returns the COLOR_PAIRS available, if the curses library supports it.
  def color_pairs; end

  # returns COLORS
  def colors; end

  # Returns the number of columns on the screen
  def cols; end

  # Sets Cursor Visibility.
  # 0: invisible
  # 1: visible
  # 2: very visible
  def curs_set(visibility) end

  # Save the current terminal modes as the "program"
  # state for use by the Curses.reset_prog_mode
  #
  # This is done automatically by Curses.init_screen
  def def_prog_mode; end

  # Delete the character under the cursor
  def delch; end

  # Delete the line under the cursor.
  def deleteln; end

  # Refreshes the windows and lines.
  #
  # Curses.doupdate allows multiple updates with
  # more efficiency than Curses.refresh alone.
  def doupdate; end

  # Enables characters typed by the user
  # to be echoed by Curses.getch as they are typed.
  def echo; end

  # Flashs the screen, for visual alarm on the terminal, if possible;
  # otherwise it sounds the alert.
  #
  # see also Curses.beep
  def flash; end

  # Read and returns a character from the window.
  #
  # See Curses::Key to all the function KEY_* available
  def getch; end

  # Returns coordinates of the mouse.
  #
  # This will read and pop the mouse event data off the queue
  #
  # See the BUTTON*, ALL_MOUSE_EVENTS and REPORT_MOUSE_POSITION constants, to examine the mask of the event
  def getmouse; end

  # This is equivalent to a series f Curses::Window.getch calls
  def getstr; end

  # Returns +true+ or +false+ depending on whether the terminal has color capbilities.
  def has_colors?; end

  # Returns the character at the current position.
  def inch; end

  # Changes the definition of a color. It takes four arguments:
  # * the number of the color to be changed, +color+
  # * the amount of red, +r+
  # * the amount of green, +g+
  # * the amount of blue, +b+
  #
  # The value of the first argument must be between 0 and  COLORS.
  # (See the section Colors for the default color index.)  Each
  # of the last three arguments must be a value between 0 and 1000.
  # When Curses.init_color is used, all occurrences of that color
  # on the screen immediately change to the new definition.
  def init_color(color, r, g, b) end

  # Changes the definition of a color-pair.
  #
  # It takes three arguments: the number of the color-pair to be changed +pair+,
  # the foreground color number +f+, and the background color number +b+.
  #
  # If the color-pair was previously initialized, the screen is
  # refreshed and all occurrences of that color-pair are changed
  # to the new definition.
  def init_pair(pair, f, b) end

  # Initialize a standard screen
  #
  # see also Curses.stdscr
  def init_screen; end

  # Insert a character +ch+, before the cursor.
  def insch(ch) end

  # Inserts a line above the cursor, and the bottom line is lost
  def insertln; end

  # Returns the character string corresponding to key +c+
  def keyname(c) end

  # Returns the number of lines on the screen
  def lines; end

  # The Curses.mouseinterval function sets the maximum time
  # (in thousands of a second) that can elapse between press
  # and release events for them to be recognized as a click.
  #
  # Use Curses.mouseinterval(0) to disable click resolution.
  # This function returns the previous interval value.
  #
  # Use Curses.mouseinterval(-1) to obtain the interval without
  # altering it.
  #
  # The default is one sixth of a second.
  def mouseinterval(interval) end

  # Returns the +mask+ of the reportable events
  def mousemask(mask) end

  # Enable the underlying display device to translate
  # the return key into newline on input, and whether it
  # translates newline into return and line-feed on output
  # (in either case, the call Curses.addch('\n') does the
  # equivalent of return and line feed on the virtual screen).
  #
  # Initially, these translations do occur. If you disable
  # them using Curses.nonl, curses will be able to make better use
  # of the line-feed capability, resulting in faster cursor
  # motion. Also, curses will then be able to detect the return key.
  def nl; end

  # Put the terminal into normal mode (out of cbreak mode).
  #
  # See Curses.cbreak for more detail.
  def nocbreak; end
  alias nocrmode nocbreak

  # Disables characters typed by the user
  # to be echoed by Curses.getch as they are typed.
  def noecho; end

  # Disable the underlying display device to translate
  # the return key into newline on input
  #
  # See Curses.nl for more detail
  def nonl; end

  # Put the terminal out of raw mode.
  #
  # see Curses.raw for more detail
  def noraw; end

  # Returns a 2 item Array, with the foreground and
  # background color, in +pair+
  def pair_content(pair) end

  # Returns the Fixnum color pair number of attributes +attrs+.
  def pair_number(attrs) end

  # Put the terminal into raw mode.
  #
  # Raw mode is similar to Curses.cbreak mode, in that characters typed
  # are immediately passed through to the user program.
  #
  # The differences are that in raw mode, the interrupt, quit,
  # suspend, and flow control characters are all passed through
  # uninterpreted, instead of generating a signal. The behavior
  # of the BREAK key depends on other bits in the tty driver
  # that are not set by curses.
  def raw; end

  # Refreshes the windows and lines.
  def refresh; end

  # Reset the current terminal modes to the saved state
  # by the Curses.def_prog_mode
  #
  # This is done automatically by Curses.close_screen
  def reset_prog_mode; end

  # Resize the current term to Fixnum +lines+ and Fixnum +cols+
  def resizeterm(lines, cols) end
  alias resize resizeterm

  # Scrolls the current window Fixnum +num+ lines.
  # The current cursor position is not changed.
  #
  # For positive +num+, it scrolls up.
  #
  # For negative +num+, it scrolls down.
  def scrl(num) end

  # A setter for the position of the cursor,
  # using coordinates +x+ and +y+
  def setpos(y, x) end

  # Set a software scrolling region in a window.
  # +top+ and +bottom+ are lines numbers of the margin.
  #
  # If this option and Curses.scrollok are enabled, an attempt to move off
  # the bottom margin line causes all lines in the scrolling region
  # to scroll one line in the direction of the first line.
  # Only the text of the window is scrolled.
  def setscrreg(top, bottom) end

  # Enables the Normal display (no highlight)
  #
  # This is equivalent to Curses.attron(A_NORMAL)
  #
  # see also Curses::Window.attrset for additional information.
  def standend; end

  # Enables the best highlighting mode of the terminal.
  #
  # This is equivalent to Curses:Window.attron(A_STANDOUT)
  #
  # see also Curses::Window.attrset additional information
  def standout; end

  # Initializes the color attributes, for terminals that support it.
  #
  # This must be called, in order to use color attributes.
  # It is good practice to call it just after Curses.init_screen
  def start_color; end

  # The Standard Screen.
  #
  # Upon initializing curses, a default window called stdscr,
  # which is the size of the terminal screen, is created.
  #
  # Many curses functions use this window.
  def stdscr; end

  # Sets block and non-blocking reads for the window.
  # - If delay is negative, blocking read is used (i.e., waits indefinitely for input).
  # - If delay is zero, then non-blocking read is used (i.e., read returns ERR if no input is waiting).
  # - If delay is positive, then read blocks for delay milliseconds, and returns ERR if there is still no input.
  def timeout=(delay) end

  # Places +ch+ back onto the input queue to be returned by
  # the next call to Curses.getch.
  #
  # There is just one input queue for all windows.
  def ungetch(ch) end

  # It pushes a KEY_MOUSE event onto the input queue, and associates with that
  # event the given state data and screen-relative character-cell coordinates.
  #
  # The Curses.ungetmouse function behaves analogously to Curses.ungetch.
  def ungetmouse(p1) end

  # tells the curses library to use terminal's default colors.
  #
  # see also the system manual for default_colors(3)
  def use_default_colors; end

  # a container for the KEY_* values.
  #
  # See also system manual for getch(3)
  module Key
    A1 = _
    A3 = _
    B2 = _
    BACKSPACE = _
    BEG = _
    BREAK = _
    # Back tab key
    BTAB = _
    C1 = _
    C3 = _
    CANCEL = _
    CATAB = _
    CLEAR = _
    CLOSE = _
    COMMAND = _
    COPY = _
    CREATE = _
    CTAB = _
    DC = _
    DL = _
    DOWN = _
    EIC = _
    ENTER = _
    EOL = _
    EOS = _
    EXIT = _
    FIND = _
    HELP = _
    HOME = _
    IC = _
    IL = _
    LEFT = _
    LL = _
    MARK = _
    MAX = _
    MESSAGE = _
    MIN = _
    MOUSE = _
    MOVE = _
    NEXT = _
    NPAGE = _
    OPEN = _
    OPTIONS = _
    PPAGE = _
    PREVIOUS = _
    PRINT = _
    REDO = _
    REFERENCE = _
    REFRESH = _
    REPLACE = _
    RESET = _
    RESIZE = _
    RESTART = _
    RESUME = _
    RIGHT = _
    SAVE = _
    SBEG = _
    SCANCEL = _
    SCOMMAND = _
    SCOPY = _
    SCREATE = _
    SDC = _
    SDL = _
    SELECT = _
    SEND = _
    SEOL = _
    SEXIT = _
    SF = _
    SFIND = _
    SHELP = _
    SHOME = _
    SIC = _
    SLEFT = _
    SMESSAGE = _
    SMOVE = _
    SNEXT = _
    SOPTIONS = _
    SPREVIOUS = _
    SPRINT = _
    SR = _
    SREDO = _
    SREPLACE = _
    SRESET = _
    SRIGHT = _
    SRSUME = _
    SSAVE = _
    SSUSPEND = _
    STAB = _
    SUNDO = _
    SUSPEND = _
    UNDO = _
    UP = _
  end

  # == Description
  #
  # Curses::MouseEvent
  #
  # == Example
  #
  # * mouse.rb
  #     require "curses"
  #     include Curses
  #
  #     def show_message(*msgs)
  #       message = msgs.join
  #       width = message.length + 6
  #       win = Window.new(5, width,
  #                    (lines - 5) / 2, (cols - width) / 2)
  #       win.keypad = true
  #       win.attron(color_pair(COLOR_RED)){
  #         win.box(?|, ?-, ?+)
  #       }
  #       win.setpos(2, 3)
  #       win.addstr(message)
  #       win.refresh
  #       win.getch
  #       win.close
  #     end
  #
  #     init_screen
  #     start_color
  #     init_pair(COLOR_BLUE,COLOR_BLUE,COLOR_WHITE)
  #     init_pair(COLOR_RED,COLOR_RED,COLOR_WHITE)
  #     crmode
  #     noecho
  #     stdscr.keypad(true)
  #
  #     begin
  #       mousemask(BUTTON1_CLICKED|BUTTON2_CLICKED|BUTTON3_CLICKED|BUTTON4_CLICKED)
  #       setpos((lines - 5) / 2, (cols - 10) / 2)
  #       attron(color_pair(COLOR_BLUE)|A_BOLD){
  #         addstr("click")
  #       }
  #       refresh
  #       while( true )
  #         c = getch
  #         case c
  #         when KEY_MOUSE
  #           m = getmouse
  #           if( m )
  #         show_message("getch = #{c.inspect}, ",
  #                      "mouse event = #{'0x%x' % m.bstate}, ",
  #                      "axis = (#{m.x},#{m.y},#{m.z})")
  #           end
  #           break
  #         end
  #       end
  #       refresh
  #     ensure
  #       close_screen
  #     end
  class MouseEvent
  end

  # == Description
  #
  # A Pad is like a Window but allows for scrolling of contents that cannot
  # fit on the screen.  Pads do not refresh automatically, use Pad#refresh
  # or Pad#noutrefresh instead.
  class Pad < Window
    # Contruct a new Curses::Pad with constraints of +height+ lines, +width+
    # columns
    def initialize(height, width) end

    # Refreshes the pad.  +pad_minrow+ and pad_mincol+ define the upper-left
    # corner of the rectangle to be displayed.  +screen_minrow+, +screen_mincol+,
    # +screen_maxrow+, +screen_maxcol+ define the edges of the rectangle to be
    # displayed on the screen.
    def noutrefresh(pad_minrow, pad_mincol, screen_minrow, screen_mincol, screen_maxrow, screen_maxcol) end

    # Refreshes the pad.  +pad_minrow+ and pad_mincol+ define the upper-left
    # corner of the rectangle to be displayed.  +screen_minrow+, +screen_mincol+,
    # +screen_maxrow+, +screen_maxcol+ define the edges of the rectangle to be
    # displayed on the screen.
    def refresh(pad_minrow, pad_mincol, screen_minrow, screen_mincol, screen_maxrow, screen_maxcol) end

    # Contruct a new subpad with constraints of +height+ lines, +width+ columns,
    # begin at +begin_x+ line, and +begin_y+ columns on the pad.
    def subpad(height, width, begin_x, begin_y) end
  end

  # == Description
  #
  # The means by which to create and manage frames or windows.
  # While there may be more than one window at a time, only one window
  # will receive input.
  #
  # == Usage
  #
  #   require 'curses'
  #
  #   Curses.init_screen()
  #
  #   my_str = "LOOK! PONIES!"
  #   win = Curses::Window.new( 8, (my_str.length + 10),
  #                             (Curses.lines - 8) / 2,
  #                             (Curses.cols - (my_str.length + 10)) / 2 )
  #   win.box("|", "-")
  #   win.setpos(2,3)
  #   win.addstr(my_str)
  #   # or even
  #   win << "\nORLY"
  #   win << "\nYES!! " + my_str
  #   win.refresh
  #   win.getch
  #   win.close
  class Window < Data
    # Contruct a new Curses::Window with constraints of
    # +height+ lines, +width+ columns, begin at +top+ line, and begin +left+ most column.
    #
    # A new window using full screen is called as
    #      Curses::Window.new(0,0,0,0)
    def initialize(height, width, top, left) end

    # Add String +str+ to the current string.
    #
    # See also Curses::Window.addstr
    def <<(p1) end

    # Add a character +ch+, with attributes, to the window, then advance the cursor.
    #
    # see also the system manual for curs_addch(3)
    def addch(ch) end

    # add a string of characters +str+, to the window and advance cursor
    def addstr(str) end

    # Turns on the named attributes +attrs+ without affecting any others.
    #
    # See also Curses::Window.attrset
    def attroff(attrs) end

    # Turns off the named attributes +attrs+
    # without turning any other attributes on or off.
    #
    # See also Curses::Window.attrset
    def attron(attrs) end

    # Sets the current attributes of the given window to +attrs+.
    #
    # The following video attributes, defined in <curses.h>, can
    # be passed to the routines Curses::Window.attron, Curses::Window.attroff,
    # and Curses::Window.attrset, or OR'd with the characters passed to addch.
    #   A_NORMAL        Normal display (no highlight)
    #   A_STANDOUT      Best highlighting mode of the terminal.
    #   A_UNDERLINE     Underlining
    #   A_REVERSE       Reverse video
    #   A_BLINK         Blinking
    #   A_DIM           Half bright
    #   A_BOLD          Extra bright or bold
    #   A_PROTECT       Protected mode
    #   A_INVIS         Invisible or blank mode
    #   A_ALTCHARSET    Alternate character set
    #   A_CHARTEXT      Bit-mask to extract a character
    #   COLOR_PAIR(n)   Color-pair number n
    #
    # TODO: provide some examples here.
    #
    # see also system manual curs_attr(3)
    def attrset(attrs) end

    # A getter for the beginning column (X coord) of the window
    def begx; end

    # A getter for the beginning line (Y coord) of the window
    def begy; end

    # Set the background of the current window
    # and apply character Integer +ch+ to every character.
    #
    # see also Curses.bkgd
    def bkgd(ch) end

    # Manipulate the background of the current window
    # with character Integer +ch+
    #
    # see also Curses.bkgdset
    def bkgdset(ch) end

    # set the characters to frame the window in.
    # The vertical +vert+ and horizontal +hor+ character.
    #
    #      win = Curses::Window.new(5,5,5,5)
    #      win.box(?|, ?-)
    def box(vert, hor) end

    # Clear the window.
    def clear; end

    # Deletes the window, and frees the memory
    def close; end

    # Clear the window to the end of line, that the cursor is currently on.
    def clrtoeol; end

    # Sets the current color of the given window to the
    # foreground/background combination described by the Fixnum +col+.
    def color_set(col) end

    # A getter for the current column (X coord) of the window
    def curx; end

    # A getter for the current line (Y coord) of the window
    def cury; end

    # Delete the character under the cursor
    def delch; end

    # Delete the line under the cursor.
    def deleteln; end

    # Returns an Interer (+ch+) for the character property in the current window.
    def getbkgd; end

    # Read and returns a character from the window.
    #
    # See Curses::Key to all the function KEY_* available
    def getch; end

    # This is equivalent to a series f Curses::Window.getch calls
    def getstr; end

    # If +bool+ is +true+ curses considers using the hardware insert/delete
    # line feature of terminals so equipped.
    #
    # If +bool+ is +false+, disables use of line insertion and deletion.
    # This option should be enabled only if the application needs insert/delete
    # line, for example, for a screen editor.
    #
    # It is disabled by default because insert/delete line tends to be visually
    # annoying when used in applications where it is not really needed.
    # If insert/delete line cannot be used, curses redraws the changed portions of all lines.
    def idlok(bool) end

    # Returns the character at the current position of the window.
    def inch; end

    # Insert a character +ch+, before the cursor, in the current window
    def insch(ch) end

    # Inserts a line above the cursor, and the bottom line is lost
    def insertln; end

    # Enables the keypad of the user's terminal.
    #
    # If enabled (+bool+ is +true+), the user can press a function key
    # (such as an arrow key) and wgetch returns a single value representing
    # the function key, as in KEY_LEFT.  If disabled (+bool+ is +false+),
    # curses does not treat function keys specially and the program has to
    # interpret the escape sequences itself.  If the keypad in the terminal
    # can be turned on (made to transmit) and off (made to work locally),
    # turning on this option causes the terminal keypad to be turned on when
    # Curses::Window.getch is called.
    #
    # The default value for keypad is false.
    def keypad(bool) end
    alias keypad= keypad

    # A getter for the maximum columns for the window
    def maxx; end

    # A getter for the maximum lines for the window
    def maxy; end

    # Moves the window so that the upper left-hand corner is at position (+y+, +x+)
    def move(y, x) end

    # When in no-delay mode Curses::Window#getch is a non-blocking call.  If no
    # input is ready #getch returns ERR.
    #
    # When in delay mode (+bool+ is +false+ which is the default),
    # Curses::Window#getch blocks until a key is pressed.
    def nodelay=(bool) end

    # Refreshes the windows and lines.
    #
    # Curses::Window.noutrefresh allows multiple updates with
    # more efficiency than Curses::Window.refresh alone.
    def noutrefresh; end

    # Refreshes the windows and lines.
    def refresh; end

    # Resize the current window to Fixnum +lines+ and Fixnum +cols+
    def resize(lines, cols) end

    # Scrolls the current window Fixnum +num+ lines.
    # The current cursor position is not changed.
    #
    # For positive +num+, it scrolls up.
    #
    # For negative +num+, it scrolls down.
    def scrl(num) end

    # Scrolls the current window up one line.
    def scroll; end

    # Controls what happens when the cursor of a window
    # is moved off the edge of the window or scrolling region,
    # either as a result of a newline action on the bottom line,
    # or typing the last character of the last line.
    #
    # If disabled, (+bool+ is false), the cursor is left on the bottom line.
    #
    # If enabled, (+bool+ is true), the window is scrolled up one line
    # (Note that to get the physical scrolling effect on the terminal,
    # it is also necessary to call Curses::Window.idlok)
    def scrollok(bool) end

    # A setter for the position of the cursor
    # in the current window,
    # using coordinates +x+ and +y+
    def setpos(y, x) end

    # Set a software scrolling region in a window.
    # +top+ and +bottom+ are lines numbers of the margin.
    #
    # If this option and Curses::Window.scrollok are enabled, an attempt to move
    # off the bottom margin line causes all lines in the scrolling region to
    # scroll one line in the direction of the first line.  Only the text of the
    # window is scrolled.
    def setscrreg(top, bottom) end

    # Enables the Normal display (no highlight)
    #
    # This is equivalent to Curses::Window.attron(A_NORMAL)
    #
    # see also Curses::Window.attrset
    def standend; end

    # Enables the best highlighting mode of the terminal.
    #
    # This is equivalent to Curses::Window.attron(A_STANDOUT)
    #
    # see also Curses::Window.attrset
    def standout; end

    # Contruct a new subwindow with constraints of
    # +height+ lines, +width+ columns, begin at +top+ line, and begin +left+ most column.
    def subwin(height, width, top, left) end

    # Sets block and non-blocking reads for the window.
    # - If delay is negative, blocking read is used (i.e., waits indefinitely for input).
    # - If delay is zero, then non-blocking read is used (i.e., read returns ERR if no input is waiting).
    # - If delay is positive, then read blocks for delay milliseconds, and returns ERR if there is still no input.
    def timeout=(delay) end
  end
end
