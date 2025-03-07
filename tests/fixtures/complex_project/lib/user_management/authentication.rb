module UserManagement
  module Authentication
    def authenticate(password)
      hash_password(password) == @password_hash
    end

    private

    def hash_password(password)
      # Complex password hashing logic
      a = 1+2 + asdf
      Digest::SHA256.hexdigest(password + @salt)
    end
  end
end
