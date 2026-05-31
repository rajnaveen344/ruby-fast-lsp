# frozen_string_literal: true

# <code>WIN32OLE_TYPE</code> objects represent OLE type libarary information.
class WIN32OLE_TYPE
  # Returns array of WIN32OLE_TYPE objects defined by the <i>typelib</i> type library.
  def self.ole_classes(typelib) end

  # Returns array of ProgID.
  def self.progids; end

  # Returns array of type libraries.
  def self.typelibs; end

  # Returns a new WIN32OLE_TYPE object.
  # The first argument <i>typelib</i> specifies OLE type library name.
  # The second argument specifies OLE class name.
  #
  #     WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Application')
  #         # => WIN32OLE_TYPE object of Application class of Excel.
  def initialize(typelib, ole_class) end

  # Returns GUID.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Application')
  #   puts tobj.guid  # => {00024500-0000-0000-C000-000000000046}
  def guid; end

  # Returns helpcontext. If helpcontext is not found, then returns nil.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
  #    puts tobj.helpfile # => 131185
  def helpcontext; end

  # Returns helpfile path. If helpfile is not found, then returns nil.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
  #    puts tobj.helpfile # => C:\...\VBAXL9.CHM
  def helpfile; end

  # Returns help string.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Internet Controls', 'IWebBrowser')
  #   puts tobj.helpstring # => Web Browser interface
  def helpstring; end

  # Returns major version.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Word 10.0 Object Library', 'Documents')
  #    puts tobj.major_version # => 8
  def major_version; end

  # Returns minor version.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Word 10.0 Object Library', 'Documents')
  #    puts tobj.minor_version # => 2
  def minor_version; end

  # Returns OLE type name.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Application')
  #    puts tobj.name  # => Application
  def name; end
  alias to_s name

  # Returns array of WIN32OLE_METHOD objects which represent OLE method defined in
  # OLE type library.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
  #   methods = tobj.ole_methods.collect{|m|
  #     m.name
  #   }
  #   # => ['Activate', 'Copy', 'Delete',....]
  def ole_methods; end

  # returns type of OLE class.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Application')
  #   puts tobj.ole_type  # => Class
  def ole_type; end

  # Returns ProgID if it exists. If not found, then returns nil.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Application')
  #    puts tobj.progid  # =>   Excel.Application.9
  def progid; end

  # Returns source class when the OLE class is 'Alias'.
  #    tobj =  WIN32OLE_TYPE.new('Microsoft Office 9.0 Object Library', 'MsoRGBType')
  #    puts tobj.src_type # => I4
  def src_type; end

  # Returns number which represents type.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Word 10.0 Object Library', 'Documents')
  #   puts tobj.typekind # => 4
  def typekind; end

  # Returns array of WIN32OLE_VARIABLE objects which represent variables
  # defined in OLE class.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
  #    vars = tobj.variables
  #    vars.each do |v|
  #      puts "#{v.name} = #{v.value}"
  #    end
  #
  #    The result of above sample script is follows:
  #      xlChart = -4109
  #      xlDialogSheet = -4116
  #      xlExcel4IntlMacroSheet = 4
  #      xlExcel4MacroSheet = 3
  #      xlWorksheet = -4167
  def variables; end

  # Returns true if the OLE class is public.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Application')
  #   puts tobj.visible  # => true
  def visible?; end
end
