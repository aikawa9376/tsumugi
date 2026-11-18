import unittest
from normalize_licenses import normalize

SEP = '\n' + '=' * 80 + '\n'


class LicenseRecords(unittest.TestCase):
    def test_order_does_not_change_the_notices(self):
        a = 'MIT (MIT)\n\nUsed by:\n  - z 1\n  - a 1\n\nCopyright A\nPermission text.\n\n'
        b = 'BSD (BSD-2-Clause)\n\nUsed by:\n  - b 2\n\nCopyright B\nRedistribution text.\n\n'
        left = normalize('Header\n' + SEP + a + SEP + b)
        right = normalize('Header\n' + SEP + b + SEP + a.replace('  - z 1\n  - a 1', '  - a 1\n  - z 1'))
        self.assertEqual(left, right)
        self.assertEqual(normalize(left), left)
        self.assertIn('Copyright A\nPermission text.', left)
        self.assertIn('Copyright B\nRedistribution text.', left)

    def test_changed_licensing_and_missing_crates_still_differ(self):
        text = 'Header\n' + SEP + 'MIT\n\nUsed by:\n  - a 1\n\nCopyright A\n'
        self.assertNotEqual(normalize(text), normalize(text.replace('Copyright A', 'Copyright B')))
        self.assertNotEqual(normalize(text), normalize(text.replace('  - a 1\n', '')))


if __name__ == '__main__':
    unittest.main()
