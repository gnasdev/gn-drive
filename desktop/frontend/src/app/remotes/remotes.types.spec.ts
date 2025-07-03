import { 
  RemoteType, 
  REMOTE_TYPE_OPTIONS, 
  isValidRemoteType, 
  isRemoteFormData 
} from './remotes.types';

describe('RemoteTypes', () => {
  describe('REMOTE_TYPE_OPTIONS', () => {
    it('should contain all expected remote types', () => {
      const expectedTypes: RemoteType[] = [
        'drive',
        'dropbox', 
        'onedrive',
        'yandex',
        'gphotos',
        'iclouddrive'
      ];

      const actualTypes = REMOTE_TYPE_OPTIONS.map(option => option.value);
      
      expectedTypes.forEach(type => {
        expect(actualTypes).toContain(type);
      });
    });

    it('should have proper labels for new providers', () => {
      const gPhotosOption = REMOTE_TYPE_OPTIONS.find(opt => opt.value === 'gphotos');
      const iCloudOption = REMOTE_TYPE_OPTIONS.find(opt => opt.value === 'iclouddrive');

      expect(gPhotosOption).toBeDefined();
      expect(gPhotosOption?.label).toBe('Google Photos');
      expect(gPhotosOption?.icon).toBe('photo_library');

      expect(iCloudOption).toBeDefined();
      expect(iCloudOption?.label).toBe('iCloud Drive');
      expect(iCloudOption?.icon).toBe('cloud_upload');
    });

    it('should have unique values', () => {
      const values = REMOTE_TYPE_OPTIONS.map(option => option.value);
      const uniqueValues = [...new Set(values)];
      
      expect(values.length).toBe(uniqueValues.length);
    });

    it('should have non-empty labels and icons', () => {
      REMOTE_TYPE_OPTIONS.forEach(option => {
        expect(option.label).toBeTruthy();
        expect(option.icon).toBeTruthy();
        expect(option.value).toBeTruthy();
      });
    });
  });

  describe('isValidRemoteType', () => {
    it('should return true for valid remote types', () => {
      const validTypes = [
        'drive',
        'dropbox',
        'onedrive', 
        'yandex',
        'gphotos',
        'iclouddrive'
      ];

      validTypes.forEach(type => {
        expect(isValidRemoteType(type)).toBe(true);
      });
    });

    it('should return false for invalid remote types', () => {
      const invalidTypes = [
        'invalid',
        'nonexistent',
        '',
        'DRIVE',
        'Drive',
        null,
        undefined
      ];

      invalidTypes.forEach(type => {
        expect(isValidRemoteType(type as any)).toBe(false);
      });
    });

    it('should include new providers in validation', () => {
      expect(isValidRemoteType('gphotos')).toBe(true);
      expect(isValidRemoteType('iclouddrive')).toBe(true);
    });
  });

  describe('isRemoteFormData', () => {
    it('should return true for valid RemoteFormData', () => {
      const validData = [
        { name: 'test', type: 'drive' as RemoteType },
        { name: 'my-photos', type: 'gphotos' as RemoteType },
        { name: 'my-icloud', type: 'iclouddrive' as RemoteType },
        { name: 'dropbox-backup', type: 'dropbox' as RemoteType }
      ];

      validData.forEach(data => {
        expect(isRemoteFormData(data)).toBe(true);
      });
    });

    it('should return false for invalid RemoteFormData', () => {
      const invalidData = [
        null,
        undefined,
        {},
        { name: 'test' },
        { type: 'drive' },
        { name: '', type: 'drive' },
        { name: 'test', type: 'invalid' },
        { name: 123, type: 'drive' },
        { name: 'test', type: null }
      ];

      invalidData.forEach(data => {
        expect(isRemoteFormData(data)).toBe(false);
      });
    });

    it('should validate new provider types correctly', () => {
      expect(isRemoteFormData({ name: 'test', type: 'gphotos' })).toBe(true);
      expect(isRemoteFormData({ name: 'test', type: 'iclouddrive' })).toBe(true);
    });
  });

  describe('RemoteType type safety', () => {
    it('should enforce type safety at compile time', () => {
      // This test ensures TypeScript compilation catches invalid types
      const validType: RemoteType = 'gphotos';
      const anotherValidType: RemoteType = 'iclouddrive';
      
      expect(validType).toBe('gphotos');
      expect(anotherValidType).toBe('iclouddrive');
      
      // The following would cause TypeScript compilation errors:
      // const invalidType: RemoteType = 'invalid';
      // const anotherInvalidType: RemoteType = 'unknown';
    });
  });
});
