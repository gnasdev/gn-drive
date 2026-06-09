package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

func newCompletionCmd() *cobra.Command {
	var shell string
	cmd := &cobra.Command{
		Use:   "completion [bash|zsh|fish|powershell]",
		Short: "Generate shell completion scripts",
		Long: `Generate and install shell completion for gn-drive.

Bash:
  $ source <(gn-drive completion bash)
  # Add to ~/.bashrc for permanent installation

Zsh:
  $ gn-drive completion zsh > "${fpath[1]}/_gn-drive"

Fish:
  $ gn-drive completion fish > ~/.config/fish/completions/gn-drive.fish

PowerShell:
  $ gn-drive completion powershell > gn-drive.ps1`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			shell = args[0]
			root := cmd.Root()
			switch shell {
			case "bash":
				return root.GenBashCompletion(os.Stdout)
			case "zsh":
				return root.GenZshCompletion(os.Stdout)
			case "fish":
				return root.GenFishCompletion(os.Stdout, true)
			case "powershell":
				return root.GenPowerShellCompletionWithDesc(os.Stdout)
			default:
				return fmt.Errorf("unsupported shell: %q (want bash|zsh|fish|powershell)", shell)
			}
		},
	}
	return cmd
}