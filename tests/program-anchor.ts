import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { ProgramAnchor } from '../target/types/program_anchor';

describe('program-anchor', () => {

  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.Provider.env());

  const program = anchor.workspace.ProgramAnchor as Program<ProgramAnchor>;

  it('Is initialized!', async () => {
    // Add your test here.
    const tx = await program.rpc.initialize({});
    console.log("Your transaction signature", tx);
  });
});
