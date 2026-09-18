#!/usr/bin/env bash
# Fail when crates/ui/src paints text_color with stacked alpha on a theme
# paper (text / text_muted / text_faint) without `// a11y-ok: <reason>` on
# the same line or the line immediately above. Hsla::opacity multiplies.
# The whole text_color(...) argument is scanned, so rustfmt-wrapped
# conditionals spanning several lines are caught too.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/crates/ui/src"

find "$SRC" -name '*.rs' -print0 | sort -z | xargs -0 perl -e '
  my ($root, $violations) = (shift @ARGV, 0);
  my $marker = qr{//\s*a11y-ok:\s*\S};
  for my $file (@ARGV) {
    open my $fh, "<", $file or die "$file: $!";
    my @lines = <$fh>;
    close $fh;
    my $src = join "", @lines;
    (my $rel = $file) =~ s{^\Q$root\E/}{};
    my %seen;
    while ($src =~ /text_color\(/g) {
      my ($start, $depth, $i) = (pos($src), 1, pos($src));
      while ($i < length($src) && $depth > 0) {
        my $c = substr($src, $i++, 1);
        $depth++ if $c eq "(";
        $depth-- if $c eq ")";
      }
      my $arg = substr($src, $start, $i - $start);
      while ($arg =~ /theme\.text(?:_muted|_faint)?\.opacity\(/g) {
        my $at = $start + $-[0];
        my $lineno = (substr($src, 0, $at) =~ tr/\n//) + 1;
        next if $seen{$lineno}++;
        my $line = $lines[$lineno - 1];
        next if $line =~ $marker;
        next if $lineno > 1 && $lines[$lineno - 2] =~ $marker;
        chomp $line;
        print "$rel:$lineno:$line\n";
        $violations++;
      }
    }
  }
  if ($violations) {
    print STDERR "lint-text-alpha: $violations site(s) stack opacity on theme text paper without // a11y-ok:\n";
    exit 1;
  }
' "$ROOT"
